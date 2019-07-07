use std::collections::HashSet;
use std::fmt::Write;
use std::io::{self, Read};
use std::path::PathBuf;

use ispell::SpellLauncher;
use lettre::file::FileTransport;
use lettre::sendmail::SendmailTransport;
use lettre::Transport;
use lettre_email::{EmailBuilder, Mailbox, MimeMultipartType, PartBuilder};
use mailparse::*;
use select::document::Document;
use select::predicate::Name;
use serde_derive::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct Config {
    lang: String,
    words: Words,
    email: EmailConfig,
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields, default)]
struct Words {
    allow: HashSet<String>,
    deny: HashSet<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct EmailConfig {
    max_size_kb: Option<u64>,
    to: EmailAddr,
    from: EmailAddr,
    return_path: String,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct EmailAddr {
    name: Option<String>,
    address: String,
}

impl EmailAddr {
    fn to_mailbox(&self) -> Mailbox {
        if let Some(ref name) = self.name {
            Mailbox::new_with_name(name.to_string(), self.address.clone())
        } else {
            Mailbox::new(self.address.clone())
        }
    }
}

fn htmlentities(txt: &str) -> String {
    txt.matches(|_| true)
        .map(|ch| match ch {
            ">" => "&gt;",
            "<" => "&lt;",
            "&" => "&amp;",
            "'" => "&#39;",
            "\"" => "&quot;",
            _ => ch,
        })
        .collect()
}

fn main() -> Result<(), String> {
    let config_file: PathBuf = std::env::args_os()
        .nth(1)
        .unwrap_or_else(|| "spellcheck.toml".into())
        .into();

    let config = std::fs::read_to_string(&config_file)
        .map_err(|e| format!("Error opening {}: {}", &config_file.display(), e))?;
    let config: Config = toml::from_str(&config)
        .map_err(|e| format!("Error parsing {}: {}", &config_file.display(), e))?;

    let mut input = Vec::new();
    io::stdin()
        .take(config.email.max_size_kb.unwrap_or(128) * 1024)
        .read_to_end(&mut input)
        .map_err(|e| format!("stdin: {}", e))?;

    let mail = parse_mail(&input).map_err(|e| format!("email parse: {}", e))?;

    let mut speller = SpellLauncher::new()
        .aspell()
        .dictionary(config.lang.clone())
        .launch()
        .map_err(|e| format!("aspell start: {}", e))?;

    for word in &config.words.allow {
        speller
            .add_word(word)
            .map_err(|e| format!("aspell add word: {}", e))?;
    }

    // fall back to raw input?
    let body = mail
        .get_body()
        .map_err(|e| format!("email lacks body: {}", e))?;

    let doc = Document::from(&body[..]);

    let mut out = String::new();

    let header = r#"<html>
  <head>
    <meta charset='UTF-8'>
    <style>
      body {
        line-height: 1.3;
        font-family: "Times New Roman", Times, serif;
      }

      h1, h2 {
        font-family: sans-serif;
        font-weight: normal;
        border-bottom: 2px solid black;
      }

      p {
        margin-left: 0.5em;
        margin-right: 0.5em;
      }

      mark {
        background-color: rgba(128,0,128, 0.3);
        color: black;
        color-adjust: exact;
        -webkit-print-color-adjust: exact;
      }

      @media screen {
        section {
          max-width: 48em;
          margin: auto;
        }
      }

      @page {
        margin: 2cm;
      }
    </style>
  </head>
  <body>
"#;
    out.push_str(header);

    let mut first = true;

    // 123formbuilder's HTML is dubious at best.  Might be better to switch to
    // regexp string mangling, since that's basically what they're doing in reverse.
    for row in doc.find(Name("tr")) {
        let mut cols = row.find(Name("td"));

        let question = cols
            .next()
            .ok_or_else(|| "Couldn't find question".to_string())?
            .text();
        let answer = cols
            .next()
            .ok_or_else(|| "Couldn't find answer".to_string())?
            .text();

        // squish multiple blank lines.
        let answer = answer
            .lines()
            .filter(|s| !s.is_empty())
            .collect::<Vec<&str>>()
            .join("\n\n");

        let corrected = answer
            .lines()
            .map(|line| {
                // Work word-by-word to make marking them easier.  Sadly loses things
                // like double-spaces.  Also problematic with incorrect whitespace
                // with punctuation.
                line.split_whitespace()
                    .map(|word| {
                        let trimmed_word = word.trim_matches(|ch: char| ch.is_ascii_punctuation());
                        let errors = speller.check(trimmed_word).expect("Spellcheck error");

                        if !config.words.deny.contains(trimmed_word) && (errors.is_empty()) {
                            htmlentities(word)
                        } else {
                            format!("<mark>{}</mark>", htmlentities(word))
                        }
                    })
                    .collect::<Vec<String>>()
                    .join(" ")
            })
            .collect::<Vec<String>>()
            .join("<br>\n");

        let ans = match &question[..] {
            "Name" | "Date" => htmlentities(&answer),
            _ => corrected,
        };

        let hlevel = if first { 1 } else { 2 };
        first = false;
        writeln!(
            &mut out,
            "<section>\n<h{}>{}</h{}>\n<p>{}</p>\n</section>",
            hlevel,
            htmlentities(&question),
            hlevel,
            ans
        )
        .ok();
    }

    out.push_str("</body></html>");

    let encoded_body = base64::encode(&out);
    let attachment = PartBuilder::new()
        .body(encoded_body)
        .header((
            "Content-Disposition",
            "attachment; filename=\"spellchecked.html\"",
        ))
        .header(("Content-Type", "text/html"))
        .header(("Content-Transfer-Encoding", "base64"))
        .build();

    let orig_subject = mail
        .headers
        .get_first_value("Subject")
        .map_err(|e| format!("Failed to get Subject: {}", e))?
        .unwrap_or_else(|| "<no subject>".to_string());

    let fwd = EmailBuilder::new()
        .to(config.email.to.to_mailbox())
        .from(config.email.from.to_mailbox())
        .subject(format!("[SPELL]: {}", orig_subject))
        .header(("Return-Path", config.email.return_path))
        .message_type(MimeMultipartType::Mixed)
        .html(body)
        .child(attachment)
        .build()
        .map_err(|e| format!("Failed to build email: {}", e))?;

    if config.email.dry_run {
        println!("{}", &out);
        FileTransport::new(".")
            .send(fwd.into())
            .expect("Failed writing test message");
    } else {
        SendmailTransport::new()
            .send(fwd.into())
            .map_err(|e| format!("Failed to send email: {}", e))?;
    };

    Ok(())
}
