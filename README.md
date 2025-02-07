This project aims to read from issues and then create and manage its own PRs, to completion.

# Running

build the similarity searcher:

```sh
(cd pr-similarity-util && cargo build -r)
```

Run the solver to completion:

```sh
./solve https://github.com/block/goose/issues/1022 [--reset]
```

This will keep a local copy of the project in the repo dir (--reset will clear that out) - it works out what the project is from the issue.

# Design

- pr-similarity-search: utility to quickly search closed successful PRs to point goose to where it should run in the code using BM25
- check\*.sh: helper scripts to quickly check state of issues/ci/PR
- instructions-starting.txt: what goose uses when there is no existing PR
- instructions-iterating.txt: what goose uses when iterating on a PR that is not yet successful or has outstanding unaddressed `@goose` comments.

as a picture (not all implemented yet):
![image](https://github.com/user-attachments/assets/8e5577eb-8371-423a-b5ba-a4e144f3de37)

