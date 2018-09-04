
extern crate mailparse;
extern crate select;
extern crate ispell;

use mailparse::*;

use select::document::Document;
use select::predicate::Name;

use ispell::SpellLauncher;

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
        let answer = answer.lines()
                           .filter(|s| { !s.is_empty() })
                           .collect::<Vec<&str>>()
                           .join("\n\n");

        let corrected = answer.lines()
            .map(|line| {
                // Work word-by-word to make marking them easier.  Sadly loses things
                // like double-spaces.
                line.split_whitespace().map(|word|
                    {
                        let errors = speller.check(word).expect("Spellcheck error");

                        if errors.is_empty() {
                            word.to_string()
                        } else {
                            format!("<mark>{}</mark>", word)
                        }
                    }
                ).collect::<Vec<String>>().join(" ")
            }).collect::<Vec<String>>().join("<br>\n");

        println!("<section>\n<h1>{}</h1>\n<p>{}</p></section>", question, corrected);
    }
    println!("</body></html>");
}
