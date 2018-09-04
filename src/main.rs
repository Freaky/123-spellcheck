extern crate ispell;
extern crate mailparse;
extern crate select;

use mailparse::*;

use select::document::Document;
use select::predicate::Name;

use ispell::SpellLauncher;

use std::collections::HashSet;
use std::io::{self, Read};

const LANG: &str = "en_GB";

fn main() {
    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input).expect("reading input");

    let mail = parse_mail(&input).expect("parsing email");

    let mut speller = SpellLauncher::new()
        .aspell()
        .dictionary(LANG)
        .launch()
        .expect("Can't run spell checker");

    let allow_words = std::fs::read_to_string("words.allow")
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect::<HashSet<_>>();
    let deny_words = std::fs::read_to_string("words.deny")
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect::<HashSet<_>>();

    // fall back to raw input?
    let body = mail.get_body().expect("Can't extract email body");

    let doc = Document::from(&body[..]);

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
    println!("{}", header);

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
                // like double-spaces.
                line.split_whitespace()
                    .map(|word| {
                        let errors = speller.check(word).expect("Spellcheck error");

                        // XXX: need to deal with punctuation
                        if !deny_words.contains(word)
                            && (errors.is_empty() || allow_words.contains(word))
                        {
                            word.to_string()
                        } else {
                            format!("<mark>{}</mark>", word)
                        }
                    }).collect::<Vec<String>>()
                    .join(" ")
            }).collect::<Vec<String>>()
            .join("<br>\n");

        let out = match &question[..] {
            "Name" | "Date" => answer,
            _ => corrected,
        };

        println!("<section>\n<h1>{}</h1>\n<p>{}</p></section>", question, out);
    }
    println!("</body></html>");
}
