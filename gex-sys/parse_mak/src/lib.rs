use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;

#[derive(Default, Debug)]
pub struct Makefile<'a> {
    dict: HashMap<&'a str, Vec<&'a str>>,
}

#[derive(Debug, Eq, PartialEq)]
enum Token<'a> {
    Ident(&'a str),
    Var(&'a str),
}

impl<'a> Makefile<'a> {
    pub fn from_lines(mut lines: impl Iterator<Item = &'a str>) -> Self {
        let mut mk = Makefile::<'a>::default();

        let mut record: Vec<_> = vec![];
        while let Some(line) = lines.next() {
            if Self::is_comment(line) || line.trim().is_empty() {
                continue;
            }
            record.push(line);
            if !Self::ends_with_escape(line) {
                mk.parse(&record[..]);
                record.clear();
            }
        }
        mk
    }

    fn parse(&mut self, lines: &[&'a str]) {
        debug_assert!(!lines.is_empty());
        let (key, first_line_value) = lines[0].split_once('=').unwrap();
        let key: Vec<_> = key.split_ascii_whitespace().collect();
        assert_eq!(key.len(), 1);
        let key = key[0];

        let mut values = vec![];
        for line in [first_line_value].iter().chain(lines[1..].iter()) {
            // remove ending "\", if there is
            let tokens = line
                .trim_end()
                .trim_end_matches('\\')
                .trim_end()
                .split_ascii_whitespace();
            values.extend(tokens);
        }
        self.dict.insert(key, values);
    }

    fn split_variables<'k>(token: &'k str) -> Vec<Token<'k>> {
        let mut vars = vec![];
        enum State {
            Start,
            Dollar,
            LeftP,
            Var,
        }
        use State::*;
        let mut s = Start;

        let mut left = 0usize;
        let mut right = 0usize;
        let push_ident = |left, right, vars: &mut Vec<Token<'k>>| {
            if left < right {
                vars.push(Token::Ident(token.get(left..right).unwrap()));
            }
        };

        for (i, c) in token.chars().enumerate() {
            s = match s {
                Start => match c {
                    '$' => {
                        push_ident(left, right, &mut vars);
                        Dollar
                    }
                    '(' => panic!(),
                    ')' => panic!(),
                    _ => {
                        right += 1;
                        Start
                    }
                },
                Dollar => match c {
                    '(' => LeftP,
                    _ => panic!(),
                },
                LeftP => match c {
                    '(' => panic!(),
                    ')' => panic!(), // emtpy () not allowed
                    '$' => panic!(),
                    _ => {
                        left = i;
                        right = i + 1;
                        Var
                    }
                },
                Var => match c {
                    '(' => panic!(),
                    '$' => panic!(),
                    ')' => {
                        vars.push(Token::Var(token.get(left..right).unwrap()));
                        left = i + 1;
                        right = left;
                        Start
                    }
                    _ => {
                        right += 1;
                        Var
                    }
                },
            };
        }
        push_ident(left, right, &mut vars);
        assert!(matches!(s, Start), "bad format: {}", token);

        vars
    }

    pub fn visit(&self, variable_name: &str) -> Option<String> {
        let tokens = self.dict.get(&variable_name);
        if tokens.is_none() {
            return None;
        }
        let ret = tokens
            .unwrap()
            .iter()
            .map(|token| {
                Self::split_variables(token)
                    .into_iter()
                    .filter_map(|t| match t {
                        Token::Ident(s) => Some(s.to_owned()),
                        Token::Var(s) => self.visit(s),
                    })
                    .collect::<Vec<_>>()
                    .join("")
            })
            .collect::<Vec<_>>()
            .join(" ");
        Some(ret)
    }

    fn is_comment(line: &str) -> bool {
        let line = line.trim_start();
        line.starts_with('#')
    }
    fn ends_with_escape(line: &str) -> bool {
        let line = line.trim_end();
        line.ends_with('\\')
    }
}

pub fn parse_makefile(makefile: &Path) -> HashMap<String, String>
{
    let lines = BufReader::new(fs::File::open(makefile).unwrap()).lines();
    let lines: Vec<String> = lines.filter_map(|r| r.ok()).collect();
    let mk = Makefile::from_lines(lines.iter().map(|s| s.as_str()));

    let mut vars = HashMap::new();
    for k in mk.dict.keys(){
        vars.insert(k.to_string(), mk.visit(k).unwrap_or(String::default()));
    }
    vars
}

pub fn filter_ld_flags(flags: &str) -> Vec<String> {
    let mut ld_flags: Vec<String> = vec![];
    let mut go_with = false;
    for token in flags.split_ascii_whitespace() {
        if go_with {
            ld_flags.last_mut().unwrap().push_str(token);
            go_with = false;
        } else {
            if token == "-l" || token == "-L" {
                go_with = true;
            }
            ld_flags.push(token.to_owned());
        }
    }
    ld_flags
        .into_iter()
        .filter(|f| f.starts_with("-l") || f.starts_with("-L"))
        .collect()
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    pub fn test_parse() {
        let vars = parse_makefile(&Path::new("test/sample.mak"));
        let expected = "-L/home/wang/rust-apgas/target/debug/build/gex-sys-020db99797064414/out/lib -lgasnet-mpi-seq  \
  -lammpi   -L/home/wang/miniconda2/envs/myenv/lib/gcc/x86_64-pc-linux-gnu/8.4.0 -lgcc -lm ";
        assert_eq!(&vars["GASNET_LIBS"], expected);
    }
    #[test]
    pub fn test_split() {
        assert_eq!(
            Makefile::split_variables("L$(GASNET_PREFIX)/lib$(ANOTHER)ha"),
            vec![
                Token::Ident("L"),
                Token::Var("GASNET_PREFIX"),
                Token::Ident("/lib"),
                Token::Var("ANOTHER"),
                Token::Ident("ha")
            ]
        );
    }
    #[test]
    pub fn test_filter_ld_flags() {
        let ret = filter_ld_flags("-L/home/ -labc -L /tmp what -l cde what");
        assert_eq!(ret, vec!["-L/home/", "-labc", "-L/tmp", "-lcde"]);
    }
}
