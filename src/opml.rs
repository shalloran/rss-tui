//! Import and export OPML feed lists into/from rss-tui

use crate::{ExportOptions, ImportOptions};
use anyhow::{Context, Result};

pub(crate) fn import(options: ImportOptions) -> Result<()> {
    let mut conn = rusqlite::Connection::open(options.database_path)?;

    crate::rss::initialize_db(&mut conn)?;

    let opml_file =
        std::fs::File::open(options.opml_path).context("must provide a valid OPML file")?;

    let mut opml_reader = std::io::BufReader::new(opml_file);

    let opml_document =
        opml::OPML::from_reader(&mut opml_reader).context("unable to parse provided OPML file")?;

    let http_client = ureq::AgentBuilder::new()
        .timeout_read(options.network_timeout)
        .build();

    let feed_urls = get_feed_urls(&opml_document);

    let mut successful_imports = 0;
    let mut failed_imports = vec![];

    for feed_url in feed_urls {
        eprintln!(">>>>>>>>>>");
        eprintln!("{feed_url}: starting import");
        match crate::rss::subscribe_to_feed(&http_client, &mut conn, &feed_url) {
            Ok(_feed_id) => {
                eprintln!("{feed_url}: OK");
                successful_imports += 1;
            }
            Err(e) => {
                eprintln!("ERROR: {e:?}");
                failed_imports.push(feed_url);
            }
        };
        eprintln!("<<<<<<<<<<");
    }

    eprintln!();
    eprintln!("{successful_imports} feeds imported");
    eprintln!("{} feeds failed to import", failed_imports.len());

    if !failed_imports.is_empty() {
        eprintln!();

        for failed_import_url in failed_imports {
            eprintln!("{failed_import_url} failed to import");
        }
    }

    Ok(())
}

// outlines can be nested within other outlines in a tree structure,
// so we have to traverse them
fn get_feed_urls(opml_document: &opml::OPML) -> Vec<String> {
    let mut outlines_stack = opml_document.body.outlines.to_owned();
    let mut feed_urls = vec![];

    while let Some(this_outline) = outlines_stack.pop() {
        outlines_stack.extend_from_slice(&this_outline.outlines);

        if let Some(xml_url) = this_outline.xml_url {
            feed_urls.push(xml_url);
        }
    }

    feed_urls
}

pub(crate) fn export(options: ExportOptions) -> Result<()> {
    let mut conn = rusqlite::Connection::open(&options.database_path)?;

    crate::rss::initialize_db(&mut conn)?;

    let feeds = crate::rss::get_feeds(&conn)?;

    // create outlines for each feed
    let outlines: Vec<opml::Outline> = feeds
        .into_iter()
        .filter_map(|feed| {
            // only export feeds that have a feed_link (required for OPML)
            feed.feed_link.map(|feed_link| {
                opml::Outline {
                    text: feed.title.clone().unwrap_or_else(|| feed_link.clone()),
                    title: feed.title,
                    r#type: Some("rss".to_string()),
                    xml_url: Some(feed_link),
                    html_url: feed.link,
                    outlines: vec![],
                    ..Default::default()
                }
            })
        })
        .collect();

    // create OPML document
    let opml_doc = opml::OPML {
        version: "2.0".to_string(),
        head: Some(opml::Head {
            title: Some("rss-tui Feed Export".to_string()),
            date_created: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        }),
        body: opml::Body { outlines },
    };

    // write to file
    let opml_file = std::fs::File::create(&options.opml_path)
        .with_context(|| format!("unable to create OPML file at {:?}", options.opml_path))?;

    let mut opml_writer = std::io::BufWriter::new(opml_file);
    opml_doc
        .to_writer(&mut opml_writer)
        .with_context(|| "unable to write OPML document")?;

    eprintln!("Exported {} feeds to {:?}", opml_doc.body.outlines.len(), options.opml_path);

    Ok(())
}
