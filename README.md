### ask-gemini

> personal script to ask the Gemini from CLI. you might need <https://github.com/jeremychone/rust-genai/> if you google found this.

```bash
gmn <filename> # just extract some information from file
gmn -p review <filename> # to review
git show <SHA> | gmn -p review --stdin # to review from git
```

### License

MIT OR Apache-2.0
