### Valid Cases
1. Basic GitHub issue:
```bash
./solve https://github.com/block/goose/issues/1022 [--reset]
```

2. Basic JIRA ticket:
```bash
./solve https://block.atlassian.net/browse/EXPERIENCE-1076 
```

### Invalid URL Formats
3. Malformed GitHub issue URL:
```bash
./solve https://github.com/block/goose/issue/1022  # missing 's' in issues
./solve https://github.com/block/goose/issues/abc  # non-numeric issue number
./solve https://github.com/block/goose/issues/     # missing issue number
```

4. Malformed JIRA URLs:
```bash
./solve https://block.atlassian.net/browse/cash1234  # missing hyphen
./solve https://block.atlassian.net/browse/1234-CASH # wrong order
./solve https://block.atlassian.net/browse/cash-abc  # non-numeric ticket number
```

### Invalid Repository URLs
5. Malformed repository URLs with JIRA:
```bash
./solve https://block.atlassian.net/browse/CASH-1234 --repo=github.com/squareup/cash-web     # missing https://
./solve https://block.atlassian.net/browse/CASH-1234 --repo=https://github.com/squareup      # missing repo name
./solve https://block.atlassian.net/browse/CASH-1234 --repo=https://github.com/squareup/     # trailing slash
```

### Missing or Extra Arguments
6. Missing required arguments:
```bash
./solve  # no arguments
```