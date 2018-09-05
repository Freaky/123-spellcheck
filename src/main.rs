extern crate ispell;
extern crate mailparse;
extern crate lettre;
extern crate lettre_email;
extern crate select;

use mailparse::*;

use lettre::{EmailTransport, SendmailTransport};
use lettre_email::{EmailBuilder, Header};

use select::document::Document;
use select::predicate::Name;

use ispell::SpellLauncher;

use std::collections::HashSet;
use std::io::{self, Read};
use std::fmt::Write;

const LANG: &str = "en_GB";
const TO_ADDR: (&str, &str) = ("tom@hur.st", "Thomas Hurst");
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

        writeln!(&mut out,
            "<section>\n<h1>{}</h1>\n<p>{}</p>\n</section>",
            htmlentities(&question),
            ans
        ).ok();
    }

    out.push_str("</body></html>");

    let fwd = EmailBuilder::new()
        .to(TO_ADDR)
        .from(FROM_ADDR)
        .subject(format!("[SPELL]: {}", mail.headers.get_first_value("Subject").expect("Subject").unwrap_or_else(|| "<no subject>".to_string())))
        .header(Header::new("Return-Path".to_owned(), RETURN_ADDR.to_owned()))
        .html(out) // XXX: also attach the original?
        .build()
        .expect("Failed to build email");

    let mut mailer = SendmailTransport::new_with_command("./cat.sh");
    mailer.send(&fwd).expect("Error sending mail");
}
