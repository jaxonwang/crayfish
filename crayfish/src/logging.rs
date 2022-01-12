extern crate fern;
extern crate log;

use crate::meta_data;

use fern::colors::{Color, ColoredLevelConfig};
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread;

pub use log::{debug, error, info, trace, warn};

static GLOBAL_ID: AtomicI32 = AtomicI32::new(-1); // -1 as uninit

pub fn set_global_id(id: i32) {
    GLOBAL_ID.store(id, Ordering::Relaxed);
}

pub fn setup_logger() -> Result<(), fern::InitError> {
    let colors = ColoredLevelConfig::new()
        .info(Color::Green)
        .debug(Color::White);

    let env_value = match meta_data::env_with_prefix("LOG_LEVEL") {
        Ok(v) => v,
        Err(_) => "".to_string(),
    };

    // TODO upper case
    let debug_level = match env_value.as_str() {
        "trace" => log::LevelFilter::Trace,
        "debug" => log::LevelFilter::Debug,
        "info" => log::LevelFilter::Info,
        "warn" => log::LevelFilter::Warn,
        "error" => log::LevelFilter::Error,
        _ if cfg!(debug_assertions) => log::LevelFilter::Debug,
        _ => log::LevelFilter::Info,
    };
    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{date} {file}:{line} {pid}:{tid:?}@{hostname} {level} {message}",
                date = chrono::Local::now().format("%H:%M:%S.%6f"),
                file = record.file().unwrap_or("unknown"),
                line = record.line().unwrap_or(0),
                pid = GLOBAL_ID.load(Ordering::Relaxed),
                tid = thread::current().id(),
                hostname = *meta_data::HOSTNAME,
                level = colors.color(record.level()),
                message = message
            ))
        })
        .level(debug_level)
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}

pub fn pretty_table_formatter(header: Vec<String>, body: Vec<Vec<String>>) -> String {
    for row in body.iter() {
        assert_eq!(header.len(), row.len());
    }

    let mut max_content_width_by_col: Vec<usize> = vec![0; header.len()];
    for (i, name) in header.iter().enumerate() {
        if max_content_width_by_col[i] < name.len() {
            max_content_width_by_col[i] = name.len();
        }
    }
    for row in body.iter() {
        for (i, content) in row.iter().enumerate() {
            if max_content_width_by_col[i] < content.len() {
                max_content_width_by_col[i] = content.len();
            }
        }
    }

    let min_column_margin = 4usize;

    let mut final_table = String::new();

    // build table
    let bar_len =
        max_content_width_by_col.iter().sum::<usize>() + (header.len() - 1) * min_column_margin;
    let bar = "-".repeat(bar_len);
    let push_one_cell = |table: &mut String, content: &str, index: usize| {
        table.push_str(content);
        table.push_str(&" ".repeat(max_content_width_by_col[index] - content.len()));
        if index != header.len() - 1 {
            table.push_str(&" ".repeat(min_column_margin))
        }
    };

    // bar
    final_table.push_str(&bar);
    final_table.push('\n');

    // header
    for (i, name) in header.iter().enumerate() {
        push_one_cell(&mut final_table, name, i);
    }
    final_table.push('\n');

    // bar
    final_table.push_str(&bar);
    final_table.push('\n');

    // body
    for row in body.iter() {
        for (j, content) in row.iter().enumerate() {
            push_one_cell(&mut final_table, content, j);
        }
        final_table.push('\n');
    }

    // another bar
    final_table.push_str(&bar);

    final_table
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_pretty_table_formatter() {
        let header = vec!["header1", "h1", "hhhhhheader1", "header"];
        let body = vec![
            vec!["1", "1111", "111", "1"],
            vec!["22222", "1111", "111", "2"],
            vec!["1", "11", "111", "2"],
            vec!["123", "1111", "111", "2"],
        ];
        let header = header.iter().map(|&s| String::from(s)).collect();
        let body = body
            .iter()
            .map(|r| r.iter().map(|&s| s.to_owned()).collect())
            .collect();
        // println!("{}", pretty_table_formatter(header, body));
        let expected = "-----------------------------------------
header1    h1      hhhhhheader1    header
-----------------------------------------
1          1111    111             1     
22222      1111    111             2     
1          11      111             2     
123        1111    111             2     
-----------------------------------------";
        assert_eq!(expected, pretty_table_formatter(header, body));
    }
}
