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

fn load_wordlist(name: &str) -> HashSet<String> {
    std::fs::read_to_string(name)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|s| s.is_empty())
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

        println!(
            "<section>\n<h1>{}</h1>\n<p>{}</p></section>",
            htmlentities(&question),
            htmlentities(&out)
        );
    }
    println!("</body></html>");
}
