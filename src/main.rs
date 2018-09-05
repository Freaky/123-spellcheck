extern crate base64;
extern crate ispell;
extern crate lettre;
extern crate lettre_email;
extern crate mailparse;
extern crate select;

use mailparse::*;

use lettre::{EmailTransport, SendmailTransport};
use lettre_email::{EmailBuilder, PartBuilder, MimeMultipartType};

use select::document::Document;
use select::predicate::Name;

use ispell::SpellLauncher;

use std::collections::HashSet;
use std::fmt::Write;
use std::io::{self, Read};

const LANG: &str = "en_GB";
const TO_ADDR: (&str, &str) = ("tom@hur.st", "Thomas Hurst");
// const TO_ADDR: (&str, &str) = ("NR.Pollard@mbro.ac.uk", "Neil R Pollard");
const FROM_ADDR: (&str, &str) = ("spellcheck@aagh.net", "Neil's Spellchecker");
const RETURN_ADDR: &str = "noreply@aagh.net";

fn load_wordlist(name: &str) -> HashSet<String> {
    std::fs::read_to_string(name)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
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
        }).collect()
}

fn main() {
    let mut input = Vec::new();
    io::stdin()
        .take(1024 * 128)
        .read_to_end(&mut input)
        .expect("reading input");

    let mail = parse_mail(&input).expect("parsing email");

    let mut speller = SpellLauncher::new()
        .aspell()
        .dictionary(LANG)
        .launch()
        .expect("Can't run spell checker");

    let allow_words = load_wordlist("words.allow");
    let deny_words = load_wordlist("words.deny");

    for word in &allow_words {
        speller.add_word(word).expect("couldn't add allow word");
    }

    // fall back to raw input?
    let body = mail.get_body().expect("Can't extract email body");

    let doc = Document::from(&body[..]);

    let mut out = String::new();

    let header = r#"<html>
  <head>
    <meta charset='UTF-8'>
    <style>
      body {
        line-height: 1.3;
        font: Georgia, 'Times New Roman', Times, serif;
      }

      mark {
        background-color: purple;
        color: white;
      }

      @media print {
        mark {
          text-decoration: underline;
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

    // 123formbuilder's HTML is dubious at best.  Might be better to switch to
    // regexp string mangling, since that's basically what they're doing in reverse.
    for row in doc.find(Name("tr")) {
        let mut cols = row.find(Name("td"));

        let question = cols.next().expect("Couldn't find question").text();
        let answer = cols.next().expect("Couldn't find answer").text();

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

                        if !deny_words.contains(trimmed_word)
                            && (errors.is_empty() || allow_words.contains(trimmed_word))
                        {
                            htmlentities(word)
                        } else {
                            format!("<mark>{}</mark>", htmlentities(word))
                        }
                    }).collect::<Vec<String>>()
                    .join(" ")
            }).collect::<Vec<String>>()
            .join("<br>\n");

        let ans = match &question[..] {
            "Name" | "Date" => htmlentities(&answer),
            _ => corrected,
        };

        writeln!(
            &mut out,
            "<section>\n<h1>{}</h1>\n<p>{}</p>\n</section>",
            htmlentities(&question),
            ans
        ).ok();
    }

    out.push_str("</body></html>");

    let encoded_body = base64::encode(&body);
    let attachment = PartBuilder::new()
        .body(encoded_body)
        .header((
            "Content-Disposition",
            "attachment; filename=\"original.html\""
        ))
        .header((
            "Content-Type",
            "text/html"
        ))
        .header((
            "Content-Transfer-Encoding",
            "base64"
        ))
        .build();

    let fwd = EmailBuilder::new()
        .to(TO_ADDR)
        .from(FROM_ADDR)
        .subject(format!("[SPELL]: {}", mail.headers.get_first_value("Subject").expect("Subject").unwrap_or_else(|| "<no subject>".to_string())))
        .header(("Return-Path", RETURN_ADDR))
        .message_type(MimeMultipartType::Mixed)
        .html(out) // XXX: also attach the original?
        .child(attachment)
        .build()
        .expect("Failed to build email");

    let mut mailer = SendmailTransport::new_with_command("./cat.sh");
    mailer.send(&fwd).expect("Error sending mail");
}
