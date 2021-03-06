#![feature(proc_macro_hygiene, proc_macro_quote, proc_macro_span, uniform_paths)]

extern crate proc_macro;
use proc_macro::{quote, Literal, Punct, Spacing, TokenStream, TokenTree};
use std::{
    env,
    fs::{create_dir_all, read, File, OpenOptions},
    io::{BufRead, Read, Seek, SeekFrom, Write},
    iter::FromIterator,
    path::Path,
    process::Command,
};

fn is(t: &TokenTree, ch: char) -> bool {
    match t {
        TokenTree::Punct(p) => p.as_char() == ch,
        _ => false,
    }
}

#[proc_macro]
pub fn i18n(input: TokenStream) -> TokenStream {
    let span = input
        .clone()
        .into_iter()
        .next()
        .expect("Expected catalog")
        .span();
    let mut input = input.into_iter();
    let catalog = input
        .clone()
        .take_while(|t| !is(t, ','))
        .collect::<Vec<_>>();

    let file = span.source_file().path();
    let line = span.start().line;
    let out_dir = Path::new(&env::var("CARGO_TARGET_DIR").unwrap_or("target/debug".into()))
        .join("gettext_macros");
    let domain = read(out_dir.join(env::var("CARGO_PKG_NAME").expect("Please build with cargo")))
        .expect("Coudln't read domain, make sure to call init_i18n! before")
        .lines()
        .next()
        .unwrap()
        .unwrap();
    let mut pot = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(format!("po/{0}/{0}.pot", domain))
        .expect("Couldn't open .pot file");

    for _ in 0..(catalog.len() + 1) {
        input.next();
    }
    let message = input.next().unwrap();

    let mut contents = String::new();
    pot.read_to_string(&mut contents).unwrap();
    pot.seek(SeekFrom::End(0)).unwrap();

    let already_exists = contents.contains(&format!("msgid {}", message));

    let plural = match input.clone().next() {
        Some(t) => {
            if is(&t, ',') {
                input.next();
                input.next()
            } else {
                None
            }
        }
        _ => None,
    };

    let mut format_args = vec![];
    if let Some(TokenTree::Punct(p)) = input.next().clone() {
        if p.as_char() == ';' {
            loop {
                let mut tokens = vec![];
                loop {
                    if let Some(t) = input.next().clone() {
                        if !is(&t, ',') {
                            tokens.push(t);
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                if tokens.is_empty() {
                    break;
                }
                format_args.push(TokenStream::from_iter(tokens.into_iter()));
            }
        }
    }

    let mut res = TokenStream::from_iter(catalog);
    if let Some(pl) = plural {
        if !already_exists {
            pot.write_all(
                &format!(
                    r#"
# {}:{}
msgid {}
msgid_plural {}
msgstr[0] ""
"#,
                    file.to_str().unwrap(),
                    line,
                    message,
                    pl
                )
                .into_bytes(),
            )
            .expect("Couldn't write message to .pot (plural)");
        }
        let count = format_args
            .clone()
            .into_iter()
            .next()
            .expect("Item count should be specified")
            .clone();
        res.extend(quote!(
            .ngettext($message, $pl, $count)
        ))
    } else {
        if !already_exists {
            pot.write_all(
                &format!(
                    r#"
# {}:{}
msgid {}
msgstr ""
"#,
                    file.to_str().unwrap(),
                    line,
                    message
                )
                .into_bytes(),
            )
            .expect("Couldn't write message to .pot");
        }

        res.extend(quote!(
            .gettext($message)
        ))
    }
    let mut args = vec![];
    let mut first = true;
    for arg in format_args {
        if first {
            first = false;
        } else {
            args.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
        }
        args.extend(quote!(Box::new($arg)));
    }
    let mut fargs = TokenStream::new();
    fargs.extend(args);
    let res = quote!({
        ::gettext_utils::try_format($res, &[$fargs]).expect("Error while formatting message")
    });
    res
}

#[proc_macro]
pub fn init_i18n(input: TokenStream) -> TokenStream {
    let mut input = input.into_iter();
    let domain = match input.next() {
        Some(TokenTree::Literal(lit)) => lit.to_string().replace("\"", ""),
        Some(_) => panic!("Domain should be a str"),
        None => panic!("Expected a translation domain (for instance \"myapp\")"),
    };
    let mut langs = vec![];
    if let Some(t) = input.next() {
        if is(&t, ',') {
            match input.next() {
                Some(TokenTree::Ident(i)) => {
                    langs.push(i);
                    loop {
                        let next = input.next();
                        if next.is_none() || !is(&next.unwrap(), ',') {
                            break;
                        }
                        match input.next() {
                            Some(TokenTree::Ident(i)) => {
                                langs.push(i);
                            }
                            _ => panic!("Expected a language identifier"),
                        }
                    }
                }
                _ => panic!("Expected a language identifier"),
            }
        } else {
            panic!("Expected  `,`")
        }
    }

    // emit file to include
    let out_dir = Path::new(&env::var("CARGO_TARGET_DIR").unwrap_or("target/debug".into()))
        .join("gettext_macros");
    let out = out_dir.join(env::var("CARGO_PKG_NAME").expect("Please build with cargo"));
    create_dir_all(out_dir).expect("Couldn't create output dir");
    let mut out = File::create(out).expect("Metadata file couldn't be open");
    writeln!(out, "{}", domain).expect("Couldn't write domain");
    for l in langs {
        writeln!(out, "{}", l).expect("Couldn't write lang");
    }

    // write base .pot
    create_dir_all(format!("po/{}", domain)).expect("Couldn't create po dir");
    let mut pot = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(format!("po/{0}/{0}.pot", domain))
        .expect("Couldn't open .pot file");
    pot.write_all(
        &format!(
            r#"msgid ""
msgstr ""
"Project-Id-Version: {}\n"
"Report-Msgid-Bugs-To: \n"
"POT-Creation-Date: 2018-06-15 16:33-0700\n"
"PO-Revision-Date: YEAR-MO-DA HO:MI+ZONE\n"
"Last-Translator: FULL NAME <EMAIL@ADDRESS>\n"
"Language-Team: LANGUAGE <LL@li.org>\n"
"Language: \n"
"MIME-Version: 1.0\n"
"Content-Type: text/plain; charset=UTF-8\n"
"Content-Transfer-Encoding: 8bit\n"
"Plural-Forms: nplurals=INTEGER; plural=EXPRESSION;\n"
"#,
            domain
        )
        .into_bytes(),
    )
    .expect("Couldn't init .pot file");

    quote!()
}

#[proc_macro]
pub fn i18n_domain(_: TokenStream) -> TokenStream {
    let out_dir = Path::new(&env::var("CARGO_TARGET_DIR").unwrap_or("target/debug".into()))
        .join("gettext_macros");
    let domain = read(out_dir.join(env::var("CARGO_PKG_NAME").expect("Please build with cargo")))
        .expect("Coudln't read domain, make sure to call init_i18n! before")
        .lines()
        .next()
        .unwrap()
        .unwrap();
    let tok = TokenTree::Literal(Literal::string(&domain));
    quote!($tok)
}

#[proc_macro]
pub fn compile_i18n(_: TokenStream) -> TokenStream {
    let out_dir = Path::new(&env::var("CARGO_TARGET_DIR").unwrap_or("target/debug".into()))
        .join("gettext_macros");
    let file = read(out_dir.join(env::var("CARGO_PKG_NAME").expect("Please build with cargo")))
        .expect("Coudln't read domain, make sure to call init_i18n! before");
    let mut lines = file.lines();
    let domain = lines.next().unwrap().unwrap();
    let locales = lines.map(|l| l.unwrap()).collect::<Vec<_>>();

    let pot_path = Path::new("po")
        .join(domain.clone())
        .join(format!("{}.pot", domain));

    for lang in locales {
        let po_path = Path::new("po").join(format!("{}.po", lang.clone()));
        if po_path.exists() && po_path.is_file() {
            println!("Updating {}", lang.clone());
            // Update it
            Command::new("msgmerge")
                .arg("-U")
                .arg(po_path.to_str().unwrap())
                .arg(pot_path.to_str().unwrap())
                .status()
                .map(|s| {
                    if !s.success() {
                        panic!("Couldn't update PO file")
                    }
                })
                .expect("Couldn't update PO file");
        } else {
            println!("Creating {}", lang.clone());
            // Create it from the template
            Command::new("msginit")
                .arg(format!("--input={}", pot_path.to_str().unwrap()))
                .arg(format!("--output-file={}", po_path.to_str().unwrap()))
                .arg("-l")
                .arg(lang.clone())
                .arg("--no-translator")
                .status()
                .map(|s| {
                    if !s.success() {
                        panic!("Couldn't init PO file (gettext returned an error)")
                    }
                })
                .expect("Couldn't init PO file");
        }

        // Generate .mo
        let po_path = Path::new("po").join(format!("{}.po", lang.clone()));
        let mo_dir = Path::new("translations")
            .join(lang.clone())
            .join("LC_MESSAGES");
        create_dir_all(mo_dir.clone()).expect("Couldn't create MO directory");
        let mo_path = mo_dir.join(format!("{}.mo", domain));

        Command::new("msgfmt")
            .arg(format!("--output-file={}", mo_path.to_str().unwrap()))
            .arg(po_path)
            .status()
            .map(|s| {
                if !s.success() {
                    panic!("Couldn't compile translations (gettext returned an error)")
                }
            })
            .expect("Couldn't compile translations");
    }
    quote!()
}

/// Use this macro to staticaly import translations into your final binary.
///
/// ```rust,ignore
/// # //ignore because there is no translation file provided with rocket_i18n
/// # #[macro_use]
/// # extern crate rocket_i18n;
/// # use rocket_i18n::Translations;
/// let tr: Translations = include_i18n!();
/// ```
#[proc_macro]
pub fn include_i18n(_: TokenStream) -> TokenStream {
    let out_dir = Path::new(&env::var("CARGO_TARGET_DIR").unwrap_or("target/debug".into()))
        .join("gettext_macros");
    let file = read(out_dir.join(env::var("CARGO_PKG_NAME").expect("Please build with cargo")))
        .expect("Coudln't read domain, make sure to call init_i18n! before");
    let mut lines = file.lines();
    let domain = TokenTree::Literal(Literal::string(&lines.next().unwrap().unwrap()));
    let locales = lines
		.map(Result::unwrap)
		.map(|l| {
                    let lang = TokenTree::Literal(Literal::string(&l));
                    quote!{
                        ($lang, ::gettext::Catalog::parse(
                            &include_bytes!(
                                concat!(env!("CARGO_MANIFEST_DIR"), "/translations/", $lang, "/LC_MESSAGES/", $domain, ".mo")
                            )[..]
                        ).expect("Error while loading catalog")),
                    }
		}).collect::<TokenStream>();

    quote!({
        vec![
            $locales
        ]
    })
}
