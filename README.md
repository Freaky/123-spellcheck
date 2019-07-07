# 123 Spellcheck

This is a small Rust program for parsing HTML emails from 123formbuilder.com
and running GNU aspell on responses.  The results are reformulated into an email
consisting of the original HTML, and an attached HTML file with the spell-checked
output.

Configuration is via a TOML file such as the example `spellcheck.toml.example`.

## Deployment

```
-% cargo build --release
-% cp target/release/neils-spellchecker /path/to/somewhere
-% cp spellcheck.toml.example /path/to/spellcheck.toml
-% $EDITOR /path/to/spellcheck.toml
-% echo "spellcheck: |/path/to/somewhere/123-spellcheck /path/to/spellcheck.toml 2>&1 | logger" >>/etc/aliases
```

Adjust to taste: it eats an email on stdin, uses `sendmail` to send the
response, or exits non-zero with an error on stderr if something went wrong.

## Known Issues

123formbuilder's HTML emails are actually incompetent tag-soup. It looks exactly
like the posted form data is just passed through PHP's `nl2br()` without any
escaping whatsoever.

This can cause weird behaviour if the user posting the form included any HTML
markup such as less than signs.
