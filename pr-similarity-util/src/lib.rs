use anyhow::Result;
use tantivy::{
    schema::{Schema, STORED, TEXT},
    Index, IndexWriter,
    collector::TopDocs,
    query::QueryParser,
    Document,
    directory::MmapDirectory,
};

pub mod github;
use github::{GitHubClient, PullRequestDetails, PullRequestSummary};

pub struct PRSearchIndex {
    index: Index,
    writer: IndexWriter,
    query_parser: QueryParser,
    github: GitHubClient,
}

impl PRSearchIndex {
    pub fn new(owner: String, repo: String, cache_dir: &std::path::Path) -> Result<Self> {
        // Create schema
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("pr_number", TEXT | STORED);
        schema_builder.add_text_field("title", TEXT | STORED);
        schema_builder.add_text_field("description", TEXT);
        schema_builder.add_text_field("status", STORED);
        schema_builder.add_text_field("checks_status", STORED);
        schema_builder.add_text_field("files", TEXT | STORED);
        schema_builder.add_text_field("diff", TEXT);
        let schema = schema_builder.build();

        // Create/open index on disk
        std::fs::create_dir_all(cache_dir)?;
        let dir = MmapDirectory::open(cache_dir)?;
        let index = Index::open_or_create(dir, schema)?;
        let writer = index.writer(50_000_000)?; // 50MB buffer

        let description_field = index.schema()
            .get_field("description")
            .expect("description field not found");

        let query_parser = QueryParser::for_index(&index, vec![description_field]);
        let github = GitHubClient::new(owner, repo)?;

        Ok(Self {
            index,
            writer,
            query_parser,
            github,
        })
    }

    pub async fn load_recent_prs(&mut self, limit: usize, force_refresh: bool) -> Result<Vec<PullRequestSummary>> {
        if force_refresh {
            self.writer.delete_all_documents()?;
        }

        // If we have documents and aren't forcing refresh, skip loading
        let reader = self.index.reader()?;
        if !force_refresh && reader.searcher().num_docs() > 0 {
            return Ok(vec![]); // Return empty vec since we don't need the summaries
        }

        let prs = self.github.list_recent_merged_prs(limit).await?;
        
        for pr in &prs {
            let pr_details = self.github.get_pull_request_details(pr.number).await?;
            if pr_details.merged {
                self.index_pr(&pr_details)?;
            }
        }

        self.writer.commit()?;
        Ok(prs)
    }

    fn index_pr(&mut self, pr: &PullRequestDetails) -> Result<()> {
        let mut doc = Document::default();
        let schema = self.index.schema();
        
        // Get all the field accessors
        let pr_number_field = schema.get_field("pr_number").expect("pr_number field not found");
        let title_field = schema.get_field("title").expect("title field not found");
        let description_field = schema.get_field("description").expect("description field not found");
        let status_field = schema.get_field("status").expect("status field not found");
        let checks_field = schema.get_field("checks_status").expect("checks_status field not found");
        let files_field = schema.get_field("files").expect("files field not found");
        let diff_field = schema.get_field("diff").expect("diff field not found");

        // Add fields to document
        doc.add_text(pr_number_field, pr.number.to_string());
        doc.add_text(title_field, &pr.title);
        doc.add_text(description_field, &pr.description);
        doc.add_text(status_field, &pr.state);

        // Aggregate checks status
        let checks_status = if pr.workflows.iter().all(|w| w.conclusion.as_deref() == Some("success")) {
            "all_passed"
        } else if pr.workflows.iter().any(|w| w.conclusion.as_deref() == Some("failure")) {
            "some_failed"
        } else {
            "incomplete"
        };
        doc.add_text(checks_field, checks_status);

        // Extract files from diff
        let files: Vec<_> = extract_files_from_diff(&pr.diff);
        doc.add_text(files_field, files.join("\n"));
        
        // Add full diff for context
        doc.add_text(diff_field, &pr.diff);

        self.writer.add_document(doc)?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();

        let query = self.query_parser.parse_query(query)?;
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::new();
        let schema = self.index.schema();
        
        let pr_number_field = schema.get_field("pr_number").expect("pr_number field not found");
        let title_field = schema.get_field("title").expect("title field not found");
        let status_field = schema.get_field("status").expect("status field not found");
        let checks_field = schema.get_field("checks_status").expect("checks_status field not found");
        let files_field = schema.get_field("files").expect("files field not found");

        for (score, doc_address) in top_docs {
            let retrieved_doc = searcher.doc(doc_address)?;
            
            let pr_number = retrieved_doc
                .get_first(pr_number_field)
                .and_then(|v| v.as_text())
                .ok_or_else(|| anyhow::anyhow!("pr_number not found"))?
                .to_string();
            
            let title = retrieved_doc
                .get_first(title_field)
                .and_then(|v| v.as_text())
                .ok_or_else(|| anyhow::anyhow!("title not found"))?
                .to_string();

            let status = retrieved_doc
                .get_first(status_field)
                .and_then(|v| v.as_text())
                .ok_or_else(|| anyhow::anyhow!("status not found"))?
                .to_string();

            let checks_status = retrieved_doc
                .get_first(checks_field)
                .and_then(|v| v.as_text())
                .ok_or_else(|| anyhow::anyhow!("checks_status not found"))?
                .to_string();

            let files = retrieved_doc
                .get_first(files_field)
                .and_then(|v| v.as_text())
                .ok_or_else(|| anyhow::anyhow!("files not found"))?
                .to_string();

            results.push(SearchResult {
                pr_number: pr_number.parse()?,
                title,
                status,
                checks_status,
                files: files.split('\n').map(String::from).collect(),
                score,
            });
        }

        Ok(results)
    }
}

#[derive(Debug)]
pub struct SearchResult {
    pub pr_number: u64,
    pub title: String,
    pub status: String,
    pub checks_status: String,
    pub files: Vec<String>,
    pub score: f32,
}

fn extract_files_from_diff(diff: &str) -> Vec<String> {
    diff.lines()
        .filter(|line| line.starts_with("diff --git"))
        .filter_map(|line| {
            line.split(" b/").nth(1).map(String::from)
        })
        .collect()
}