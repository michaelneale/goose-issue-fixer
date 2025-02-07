This project aims to read from issues and then create and manage its own PRs, to completion.

# Running

build the similarity searcher:

```sh
(cd pr-similarity-util && cargo build -r)
```

1. Run the issue solver to completion:

```sh
./solve https://github.com/block/goose/issues/1022 [--reset]
```

This will keep a local copy of the project in the repo dir (--reset will clear that out) - it works out what the project is from the issue.
Will work on changes until happy (locally, but won't try to test) then open a PR and iterate while watching and fixing CI (currently github actions, reading logs/failures etc)
It will also respond to reviews or comments on the PR that say `@goose` and take that into account before considering the issue resolved.

2. Fix a broken PR

If you have a (non goose) PR with annoing things you just want to work:

```sh
./fix https://github.com/michaelneale/goose/pull/5
```

And it will use CI to work out how and what to fix and follow it through to completion.
(ideally this could be on most PRs so you don't have to manually fix a silly thing again)

# Design

- pr-similarity-search: utility to quickly search closed successful PRs to point goose to where it should run in the code using BM25
- check\*.sh: helper scripts to quickly check state of issues/ci/PR
- instructions-starting.txt: what goose uses when there is no existing PR
- instructions-iterating.txt: what goose uses when iterating on a PR that is not yet successful or has outstanding unaddressed `@goose` comments.

as a picture (not all implemented yet):
![image](https://github.com/user-attachments/assets/8e5577eb-8371-423a-b5ba-a4e144f3de37)
