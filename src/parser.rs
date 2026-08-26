use super::{lex, BasisError, Condition, Expression, Program, Statement, Token, TokenKind, Value};

pub fn parse(source: &str) -> Result<Program, BasisError> {
    Parser::new(source)?.parse_program()
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    fn new(source: &str) -> Result<Self, BasisError> {
        Ok(Self { tokens: lex(source)?, cursor: 0 })
    }

    fn parse_program(mut self) -> Result<Program, BasisError> {
        let (statements, _) = self.parse_block(false, false)?;
        self.skip_newlines();
        if !self.at_end() {
            return Err(self.error_here("unexpected text after program"));
        }
        Ok(Program { statements })
    }

    fn parse_block(&mut self, nested: bool, stop_at_otherwise: bool) -> Result<(Vec<Statement>, bool), BasisError> {
        let mut statements = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_end() {
                if nested {
                    return Err(self.error_here("missing `end`"));
                }
                return Ok((statements, false));
            }
            if self.is_word("end") {
                if !nested {
                    return Err(self.error_here("unexpected end"));
                }
                self.cursor += 1;
                return Ok((statements, false));
            }
            if self.is_word("otherwise") || self.is_word("else") {
                if !stop_at_otherwise {
                    return Err(self.error_here("`otherwise` or `else` must belong to a block"));
                }
                self.cursor += 1;
                self.expect_comma()?;
                self.expect_word("do")?;
                self.finish_line()?;
                return Ok((statements, true));
            }
            statements.push(self.parse_statement()?);
        }
    }

    fn parse_statement(&mut self) -> Result<Statement, BasisError> {
        if self.is_word("set") {
            return self.parse_set();
        }
        if self.is_word("save") {
            return self.parse_save();
        }
        if self.is_word("wait") {
            return self.parse_wait();
        }
        if self.starts_with_line_phrase(&["clear", "terminal"]) {
            self.cursor += 2;
            self.finish_line()?;
            return Ok(Statement::ClearTerminal);
        }
        if self.is_word("add") {
            return self.parse_list_add_or_remove(true);
        }
        if self.is_word("remove") {
            return self.parse_list_add_or_remove(false);
        }
        if self.is_word("say") {
            return self.parse_say();
        }
        if self.is_word("run") {
            return self.parse_run();
        }
        if self.is_word("create") {
            self.cursor += 1;
            self.expect_word("folder")?;
            let path = self.parse_expression_line()?;
            self.finish_line()?;
            return Ok(Statement::CreateFolder(path));
        }
        if self.is_word("copy") {
            self.cursor += 1;
            let source = self.parse_expression_until_word("to")?;
            self.expect_word("to")?;
            let destination = self.parse_expression_line()?;
            self.finish_line()?;
            return Ok(Statement::Copy { source, destination });
        }
        if self.starts_with_line_phrase(&["move", "cursor"]) {
            return self.parse_move_cursor();
        }
        if self.is_word("move") {
            self.cursor += 1;
            let source = self.parse_expression_until_word("to")?;
            self.expect_word("to")?;
            let destination = self.parse_expression_line()?;
            self.finish_line()?;
            return Ok(Statement::Move { source, destination });
        }
        if self.starts_with_line_phrase(&["hide", "cursor"]) {
            self.cursor += 2;
            self.finish_line()?;
            return Ok(Statement::HideCursor);
        }
        if self.starts_with_line_phrase(&["show", "cursor"]) {
            self.cursor += 2;
            self.finish_line()?;
            return Ok(Statement::ShowCursor);
        }
        if self.is_word("delete") {
            self.cursor += 1;
            if self.consume_word("file") {
                let path = self.parse_expression_line()?;
                self.finish_line()?;
                return Ok(Statement::DeleteFile(path));
            }
            self.expect_word("folder")?;
            let path = self.parse_expression_line()?;
            self.finish_line()?;
            return Ok(Statement::DeleteFolder(path));
        }
        if self.is_word("write") {
            self.cursor += 1;
            let content = self.parse_expression_until_phrase(&["to", "file"])?;
            self.expect_word("to")?;
            self.expect_word("file")?;
            let path = self.parse_expression_line()?;
            self.finish_line()?;
            return Ok(Statement::WriteFile { content, path });
        }
        if self.is_word("append") {
            self.cursor += 1;
            let content = self.parse_expression_until_phrase(&["to", "file"])?;
            self.expect_word("to")?;
            self.expect_word("file")?;
            let path = self.parse_expression_line()?;
            self.finish_line()?;
            return Ok(Statement::AppendFile { content, path });
        }
        if self.is_word("start") {
            self.cursor += 1;
            self.expect_word("shell")?;
            let command = self.parse_expression_line()?;
            self.finish_line()?;
            return Ok(Statement::StartShell(command));
        }
        if self.is_word("shell") {
            self.cursor += 1;
            let command = self.parse_expression_line()?;
            self.finish_line()?;
            return Ok(Statement::Shell(command));
        }
        if self.is_word("open") {
            self.cursor += 1;
            self.expect_word("file")?;
            let path = self.parse_expression_line()?;
            self.finish_line()?;
            return Ok(Statement::OpenFile(path));
        }
        if self.is_word("include") {
            self.cursor += 1;
            let path = self.parse_expression_line()?;
            self.finish_line()?;
            return Ok(Statement::Include(path));
        }
        if self.consume_word("stop") {
            self.finish_line()?;
            return Ok(Statement::Stop);
        }
        if self.consume_word("skip") {
            self.finish_line()?;
            return Ok(Statement::Skip);
        }
        if self.is_word("when") {
            return self.parse_when();
        }
        if self.is_word("try") {
            return self.parse_try();
        }
        if self.is_word("repeat") {
            return self.parse_repeat();
        }
        if self.is_word("while") {
            return self.parse_while();
        }
        if self.is_word("for") {
            return self.parse_for_each();
        }
        if self.is_word("return") {
            self.cursor += 1;
            let expression = if self.at_line_end() {
                Expression::Literal(Value::Nothing)
            } else {
                self.parse_expression_line()?
            };
            self.finish_line()?;
            return Ok(Statement::Return(expression));
        }
        if self.is_word("define") {
            return self.parse_define();
        }

        let expression = self.parse_expression_line()?;
        self.finish_line()?;
        Ok(Statement::Expression(expression))
    }

    fn parse_set(&mut self) -> Result<Statement, BasisError> {
        self.expect_word("set")?;
        let line_end = self.find_line_end(self.cursor);
        if self.starts_with_phrase(self.cursor, line_end, &["random", "seed"]) {
            self.cursor += 2;
            self.expect_word("to")?;
            let seed = self.parse_expression_line()?;
            self.finish_line()?;
            return Ok(Statement::SeedRandom(seed));
        }
        if self.consume_word("environment") {
            self.expect_word("variable")?;
            let name = self.parse_expression_until_word("to")?;
            self.expect_word("to")?;
            let value = self.parse_expression_line()?;
            self.finish_line()?;
            return Ok(Statement::SetEnvironment { name, value });
        }
        let to = self.find_first_word(self.cursor, line_end, "to").ok_or_else(|| self.error_here("expected `to` in assignment"))?;
        let path = self.parse_path_range(self.cursor, to)?;
        self.cursor = to;
        self.expect_word("to")?;
        let value = self.parse_expression_line()?;
        self.finish_line()?;
        if path.len() == 1 {
            Ok(Statement::Set { name: path[0].clone(), value })
        } else {
            Ok(Statement::SetPath { path, value })
        }
    }

    fn parse_say(&mut self) -> Result<Statement, BasisError> {
        self.expect_word("say")?;
        let line_end = self.find_line_end(self.cursor);
        if let Some(color_index) = self.find_last_word_operator(self.cursor, line_end, &["in"]) {
            let color = self.tokens_to_text(color_index + 1, line_end);
            if color_index > self.cursor && is_terminal_color(&color) {
                let expression = self.parse_expression_range(self.cursor, color_index)?;
                self.cursor = line_end;
                self.finish_line()?;
                return Ok(Statement::SayColored { expression, color });
            }
        }
        let expression = self.parse_expression_range(self.cursor, line_end)?;
        self.cursor = line_end;
        self.finish_line()?;
        Ok(Statement::Say(expression))
    }

    fn parse_move_cursor(&mut self) -> Result<Statement, BasisError> {
        self.expect_word("move")?;
        self.expect_word("cursor")?;
        self.expect_word("to")?;
        let x = self.parse_expression_until_comma()?;
        self.expect_comma()?;
        let y = self.parse_expression_line()?;
        self.finish_line()?;
        Ok(Statement::MoveCursor { x, y })
    }

    fn parse_save(&mut self) -> Result<Statement, BasisError> {
        self.expect_word("save")?;
        let value = self.parse_expression_until_phrase(&["to", "file"])?;
        self.expect_word("to")?;
        self.expect_word("file")?;
        let path = self.parse_expression_line()?;
        self.finish_line()?;
        Ok(Statement::Save { value, path })
    }

    fn parse_wait(&mut self) -> Result<Statement, BasisError> {
        self.expect_word("wait")?;
        let end = self.find_line_end(self.cursor);
        let expression_end = if end > self.cursor && self.word_at(end - 1) == Some("seconds") { end - 1 } else { end };
        let seconds = self.parse_expression_range(self.cursor, expression_end)?;
        self.cursor = end;
        self.finish_line()?;
        Ok(Statement::Wait(seconds))
    }

    fn parse_list_add_or_remove(&mut self, adding: bool) -> Result<Statement, BasisError> {
        self.cursor += 1;
        let marker = if adding { "to" } else { "from" };
        let line_end = self.find_line_end(self.cursor);
        let marker_index = self.find_last_word_operator(self.cursor, line_end, &[marker]).ok_or_else(|| self.error_here(format!("expected `{marker}` in list operation")))?;
        let value = self.parse_expression_range(self.cursor, marker_index)?;
        let path = self.parse_path_range(marker_index + 1, line_end)?;
        self.cursor = line_end;
        self.finish_line()?;
        Ok(if adding { Statement::ListAdd { path, value } } else { Statement::ListRemove { path, value } })
    }

    fn parse_run(&mut self) -> Result<Statement, BasisError> {
        self.expect_word("run")?;
        let line_end = self.find_line_end(self.cursor);
        let application_end = self.find_first_word(self.cursor, line_end, "with").unwrap_or(line_end);
        if application_end == self.cursor {
            return Err(self.error_here("expected an application after `run`"));
        }
        let application = self.tokens_to_text(self.cursor, application_end);
        self.cursor = application_end;
        let mut arguments = Vec::new();
        if self.consume_word("with") {
            while !self.at_line_end() {
                arguments.push(self.parse_expression_until_comma()?);
                if !self.consume_comma() {
                    break;
                }
            }
        }
        self.finish_line()?;
        Ok(Statement::Run { application, arguments })
    }

    fn parse_when(&mut self) -> Result<Statement, BasisError> {
        self.expect_word("when")?;
        let condition_end = self.find_first_comma(self.cursor, self.find_line_end(self.cursor)).ok_or_else(|| self.error_here("expected `when condition, do`"))?;
        let condition = self.parse_condition_range(self.cursor, condition_end)?;
        self.cursor = condition_end;
        self.expect_comma()?;
        self.expect_word("do")?;
        self.finish_line()?;
        let (body, has_otherwise) = self.parse_block(true, true)?;
        let otherwise = if has_otherwise {
            Some(self.parse_block(true, false)?.0)
        } else {
            None
        };
        Ok(Statement::When { condition, body, otherwise })
    }

    fn parse_try(&mut self) -> Result<Statement, BasisError> {
        self.expect_word("try")?;
        self.expect_comma()?;
        self.expect_word("do")?;
        self.finish_line()?;
        let (body, has_otherwise) = self.parse_block(true, true)?;
        if !has_otherwise {
            return Err(self.error_here("expected `otherwise, do` or `else, do` in try block"));
        }
        let otherwise = self.parse_block(true, false)?.0;
        Ok(Statement::Try { body, otherwise })
    }

    fn parse_repeat(&mut self) -> Result<Statement, BasisError> {
        self.expect_word("repeat")?;
        let marker = self.find_repeat_marker(self.cursor).ok_or_else(|| self.error_here("expected `repeat count times, do`"))?;
        let count = self.parse_expression_range(self.cursor, marker)?;
        self.cursor = marker;
        self.expect_word("times")?;
        self.expect_comma()?;
        self.expect_word("do")?;
        self.finish_line()?;
        let body = self.parse_block(true, false)?.0;
        Ok(Statement::Repeat { count, body })
    }

    fn parse_while(&mut self) -> Result<Statement, BasisError> {
        self.expect_word("while")?;
        let line_end = self.find_line_end(self.cursor);
        let comma = self.find_first_comma(self.cursor, line_end);
        let explicit_marker = comma.is_some_and(|index| self.word_at(index + 1) == Some("do"));
        let condition_end = if explicit_marker { comma.expect("comma exists") } else { line_end };
        let condition = self.parse_condition_range(self.cursor, condition_end)?;
        self.cursor = condition_end;
        if explicit_marker {
            self.expect_comma()?;
            self.expect_word("do")?;
        }
        self.finish_line()?;
        let body = self.parse_block(true, false)?.0;
        Ok(Statement::While { condition, body })
    }

    fn parse_for_each(&mut self) -> Result<Statement, BasisError> {
        self.expect_word("for")?;
        self.expect_word("each")?;
        let name = self.expect_any_word()?;
        self.expect_word("in")?;
        let iterable_end = self.find_first_comma(self.cursor, self.find_line_end(self.cursor)).ok_or_else(|| self.error_here("expected `for each item in collection, do`"))?;
        let iterable = self.parse_expression_range(self.cursor, iterable_end)?;
        self.cursor = iterable_end;
        self.expect_comma()?;
        self.expect_word("do")?;
        self.finish_line()?;
        let body = self.parse_block(true, false)?.0;
        Ok(Statement::ForEach { name, iterable, body })
    }

    fn parse_define(&mut self) -> Result<Statement, BasisError> {
        self.expect_word("define")?;
        let name = self.expect_any_word()?;
        let mut parameters = Vec::new();
        if self.consume_word("using") {
            while !self.is_comma_do() {
                parameters.push(self.expect_any_word()?);
                if self.is_comma_do() {
                    break;
                }
                if !self.consume_comma() {
                    return Err(self.error_here("expected a comma between function parameters"));
                }
            }
        }
        self.expect_comma()?;
        self.expect_word("do")?;
        self.finish_line()?;
        let body = self.parse_block(true, false)?.0;
        Ok(Statement::Define { name, parameters, body })
    }

    fn parse_expression_line(&mut self) -> Result<Expression, BasisError> {
        let end = self.find_line_end(self.cursor);
        let expression = self.parse_expression_range(self.cursor, end)?;
        self.cursor = end;
        Ok(expression)
    }

    fn parse_expression_until_word(&mut self, word: &str) -> Result<Expression, BasisError> {
        let line_end = self.find_line_end(self.cursor);
        let end = self.find_first_word(self.cursor, line_end, word).ok_or_else(|| self.error_here(format!("expected `{word}`")))?;
        let expression = self.parse_expression_range(self.cursor, end)?;
        self.cursor = end;
        Ok(expression)
    }

    fn parse_expression_until_phrase(&mut self, phrase: &[&str]) -> Result<Expression, BasisError> {
        let line_end = self.find_line_end(self.cursor);
        let end = self.find_first_phrase(self.cursor, line_end, phrase).ok_or_else(|| self.error_here(format!("expected `{}`", phrase.join(" "))))?;
        let expression = self.parse_expression_range(self.cursor, end)?;
        self.cursor = end;
        Ok(expression)
    }

    fn parse_expression_until_comma(&mut self) -> Result<Expression, BasisError> {
        let line_end = self.find_line_end(self.cursor);
        let end = self.find_first_comma(self.cursor, line_end).unwrap_or(line_end);
        let expression = self.parse_expression_range(self.cursor, end)?;
        self.cursor = end;
        Ok(expression)
    }

    fn parse_expression_range(&self, start: usize, end: usize) -> Result<Expression, BasisError> {
        let mut start = start;
        let mut end = end;
        while start < end && self.token_is(start, TokenKind::Newline) { start += 1; }
        while end > start && self.token_is(end - 1, TokenKind::Newline) { end -= 1; }
        if start >= end {
            return Err(self.error_at(start, "expected an expression"));
        }

        if self.is_wrapped(start, end, TokenKind::LeftParen, TokenKind::RightParen) {
            return self.parse_expression_range(start + 1, end - 1);
        }

        if let Some(index) = self.find_last_phrase(start, end, &["joined", "with"]) {
            return Ok(Expression::Join(Box::new(self.parse_expression_range(start, index)?), Box::new(self.parse_expression_range(index + 2, end)?)));
        }
        if let Some(index) = self.find_last_word_operator(start, end, &["plus", "minus"]) {
            let operator = self.word_at(index).unwrap_or_default();
            let left = self.parse_expression_range(start, index)?;
            let right = self.parse_expression_range(index + 1, end)?;
            return Ok(if operator == "plus" { Expression::Add(Box::new(left), Box::new(right)) } else { Expression::Subtract(Box::new(left), Box::new(right)) });
        }
        if let Some(index) = self.find_last_phrase(start, end, &["divided", "by"]).or_else(|| self.find_last_word_operator(start, end, &["times"])) {
            let left = self.parse_expression_range(start, index)?;
            let right_start = if self.find_last_phrase(start, end, &["divided", "by"]) == Some(index) { index + 2 } else { index + 1 };
            let right = self.parse_expression_range(right_start, end)?;
            return Ok(if self.word_at(index) == Some("times") { Expression::Multiply(Box::new(left), Box::new(right)) } else { Expression::Divide(Box::new(left), Box::new(right)) });
        }
        if let Some(index) = self.find_last_word_operator(start, end, &["at"]) {
            return Ok(Expression::At(Box::new(self.parse_expression_range(start, index)?), Box::new(self.parse_expression_range(index + 1, end)?)));
        }

        if self.starts_with_phrase(start, end, &["ask"]) {
            if self.starts_with_phrase(start, end, &["ask", "key"]) && start + 2 == end {
                return Ok(Expression::AskKey);
            }
            if start + 1 >= end {
                return Err(self.error_at(start, "expected a prompt after `ask`"));
            }
            return Ok(Expression::Ask(Box::new(self.parse_expression_range(start + 1, end)?)));
        }
        if self.starts_with_phrase(start, end, &["load", "file"]) {
            if start + 2 >= end {
                return Err(self.error_at(start, "expected a path after `load file`"));
            }
            return Ok(Expression::LoadFile(Box::new(self.parse_expression_range(start + 2, end)?)));
        }
        if self.starts_with_phrase(start, end, &["random", "choice", "from"]) {
            return Ok(Expression::RandomChoice(Box::new(self.parse_expression_range(start + 3, end)?)));
        }
        if self.starts_with_phrase(start, end, &["random", "number"]) || self.starts_with_phrase(start, end, &["random", "integer"]) {
            let integer = self.word_at(start + 1) == Some("integer");
            if start + 2 == end {
                return Ok(Expression::RandomNumber { lower: None, upper: None, integer });
            }
            if self.word_at(start + 2) != Some("from") {
                return Err(self.error_at(start + 2, "expected `from` after random number kind"));
            }
            let to = self.find_first_word(start + 3, end, "to").ok_or_else(|| self.error_at(start, "expected `to` between random number bounds"))?;
            let lower = self.parse_expression_range(start + 3, to)?;
            let upper = self.parse_expression_range(to + 1, end)?;
            return Ok(Expression::RandomNumber { lower: Some(Box::new(lower)), upper: Some(Box::new(upper)), integer });
        }
        if self.starts_with_phrase(start, end, &["minimum", "of"]) {
            let and = self.find_last_word_operator(start + 2, end, &["and"]).ok_or_else(|| self.error_at(start, "minimum needs two values joined by `and`"))?;
            return Ok(Expression::Minimum(Box::new(self.parse_expression_range(start + 2, and)?), Box::new(self.parse_expression_range(and + 1, end)?)));
        }
        if self.starts_with_phrase(start, end, &["maximum", "of"]) {
            let and = self.find_last_word_operator(start + 2, end, &["and"]).ok_or_else(|| self.error_at(start, "maximum needs two values joined by `and`"))?;
            return Ok(Expression::Maximum(Box::new(self.parse_expression_range(start + 2, and)?), Box::new(self.parse_expression_range(and + 1, end)?)));
        }
        if self.word_at(start) == Some("clamp") {
            let between = self.find_first_word(start + 1, end, "between").ok_or_else(|| self.error_at(start, "clamp needs `between` and `and`"))?;
            let and = self.find_last_word_operator(between + 1, end, &["and"]).ok_or_else(|| self.error_at(start, "clamp needs `and` between bounds"))?;
            return Ok(Expression::Clamp {
                value: Box::new(self.parse_expression_range(start + 1, between)?),
                lower: Box::new(self.parse_expression_range(between + 1, and)?),
                upper: Box::new(self.parse_expression_range(and + 1, end)?),
            });
        }

        for (phrase, constructor) in [
            (&["read", "file"][..], 0),
            (&["length", "of"][..], 1),
            (&["environment", "variable"][..], 2),
            (&["file", "exists"][..], 3),
            (&["folder", "exists"][..], 4),
            (&["list", "files", "in"][..], 5),
            (&["list", "folders", "in"][..], 6),
        ] {
            if self.starts_with_phrase(start, end, phrase) {
                let operand_start = start + phrase.len();
                if operand_start >= end {
                    return Err(self.error_at(start, format!("expected an expression after `{}`", phrase.join(" "))));
                }
                let operand = self.parse_expression_range(operand_start, end)?;
                return Ok(match constructor {
                    0 => Expression::ReadFile(Box::new(operand)),
                    1 => Expression::Length(Box::new(operand)),
                    2 => Expression::EnvironmentVariable(Box::new(operand)),
                    3 => Expression::FileExists(Box::new(operand)),
                    4 => Expression::FolderExists(Box::new(operand)),
                    5 => Expression::ListFiles(Box::new(operand)),
                    _ => Expression::ListFolders(Box::new(operand)),
                });
            }
        }
        if self.starts_with_phrase(start, end, &["current", "folder"]) && start + 2 == end {
            return Ok(Expression::CurrentFolder);
        }
        if self.starts_with_phrase(start, end, &["list", "applications"]) && start + 2 == end {
            return Ok(Expression::ListApplications);
        }
        if self.starts_with_phrase(start, end, &["terminal", "width"]) && start + 2 == end {
            return Ok(Expression::TerminalWidth);
        }
        if self.starts_with_phrase(start, end, &["terminal", "height"]) && start + 2 == end {
            return Ok(Expression::TerminalHeight);
        }

        if self.token_is(start, TokenKind::LeftBracket) && self.token_is(end - 1, TokenKind::RightBracket) {
            let values = self.split_ranges(start + 1, end - 1, TokenKind::Comma).into_iter().map(|(left, right)| self.trim_newlines(left, right)).filter_map(|(left, right)| (left < right).then_some((left, right))).map(|(left, right)| self.parse_expression_range(left, right)).collect::<Result<_, _>>()?;
            return Ok(Expression::List(values));
        }

        if self.token_is(start, TokenKind::LeftBrace) && self.token_is(end - 1, TokenKind::RightBrace) {
            let mut entries = Vec::new();
            for (left, right) in self.split_ranges(start + 1, end - 1, TokenKind::Comma).into_iter().map(|(left, right)| self.trim_newlines(left, right)) {
                if left >= right { continue; }
                let colon = self.find_first_top_level_kind(left, right, TokenKind::Colon).ok_or_else(|| self.error_at(left, "object fields need `key: value`"))?;
                let key = match &self.tokens[left].kind {
                    TokenKind::Word(value) | TokenKind::Text(value) if left + 1 == colon => value.clone(),
                    _ => return Err(self.error_at(left, "object keys must be words or text")),
                };
                let value = self.parse_expression_range(colon + 1, right)?;
                entries.push((key, value));
            }
            return Ok(Expression::Object(entries));
        }

        if let Some(index) = self.find_last_token(start, end, TokenKind::Dot) {
            if index == start || index + 1 >= end || self.word_at(index + 1).is_none() {
                return Err(self.error_at(index, "expected a property name after `.`"));
            }
            let field = self.expect_word_at(index + 1)?;
            return Ok(Expression::Field(Box::new(self.parse_expression_range(start, index)?), field));
        }

        if let Some(index) = self.find_last_word_operator(start, end, &["using"]) {
            if index == start {
                return Err(self.error_at(start, "expected a function name before `using`"));
            }
            let name = self.tokens_to_text(start, index);
            let arguments = self.split_ranges(index + 1, end, TokenKind::Comma).into_iter().filter_map(|(left, right)| (left < right).then_some((left, right))).map(|(left, right)| self.parse_expression_range(left, right)).collect::<Result<_, _>>()?;
            return Ok(Expression::Call { name, arguments });
        }

        if start + 1 == end {
            match &self.tokens[start].kind {
                TokenKind::Text(value) => return Ok(Expression::Literal(Value::Text(value.clone()))),
                TokenKind::Number(value) => return Ok(Expression::Literal(Value::Number(*value))),
                TokenKind::Word(value) if value == "true" => return Ok(Expression::Literal(Value::Boolean(true))),
                TokenKind::Word(value) if value == "false" => return Ok(Expression::Literal(Value::Boolean(false))),
                TokenKind::Word(value) if value == "nothing" => return Ok(Expression::Literal(Value::Nothing)),
                TokenKind::Word(value) => return Ok(Expression::Variable(value.clone())),
                _ => {}
            }
        }

        Err(self.error_at(start, format!("cannot understand expression `{}`", self.tokens_to_text(start, end))))
    }

    fn parse_condition_range(&self, start: usize, end: usize) -> Result<Condition, BasisError> {
        if start >= end {
            return Err(self.error_at(start, "expected a condition"));
        }
        if self.is_wrapped(start, end, TokenKind::LeftParen, TokenKind::RightParen) {
            return self.parse_condition_range(start + 1, end - 1);
        }
        if let Some(index) = self.find_last_word_operator(start, end, &["or"]) {
            return Ok(Condition::Or(Box::new(self.parse_condition_range(start, index)?), Box::new(self.parse_condition_range(index + 1, end)?)));
        }
        if let Some(index) = self.find_last_word_operator(start, end, &["and"]) {
            return Ok(Condition::And(Box::new(self.parse_condition_range(start, index)?), Box::new(self.parse_condition_range(index + 1, end)?)));
        }
        if self.word_at(start) == Some("not") {
            return Ok(Condition::Not(Box::new(self.parse_condition_range(start + 1, end)?)));
        }

        for (phrase, kind) in [
            (&["contains"][..], 0),
            (&["starts", "with"][..], 1),
            (&["ends", "with"][..], 2),
            (&["is", "not"][..], 3),
            (&["is", "greater", "than"][..], 4),
            (&["is", "less", "than"][..], 5),
            (&["is"][..], 6),
        ] {
            if let Some(index) = self.find_last_phrase(start, end, phrase) {
                let left = self.parse_expression_range(start, index)?;
                let right = self.parse_expression_range(index + phrase.len(), end)?;
                return Ok(match kind {
                    0 => Condition::Contains(left, right),
                    1 => Condition::StartsWith(left, right),
                    2 => Condition::EndsWith(left, right),
                    3 => Condition::NotEquals(left, right),
                    4 => Condition::GreaterThan(left, right),
                    5 => Condition::LessThan(left, right),
                    _ => Condition::Equals(left, right),
                });
            }
        }
        Ok(Condition::Truthy(self.parse_expression_range(start, end)?))
    }

    fn parse_path_range(&self, start: usize, end: usize) -> Result<Vec<String>, BasisError> {
        if start >= end {
            return Err(self.error_at(start, "expected a variable or property path"));
        }
        let mut path = Vec::new();
        let mut index = start;
        path.push(self.word_at(index).ok_or_else(|| self.error_at(index, "expected a variable name"))?.to_string());
        index += 1;
        while index < end {
            if !self.token_is(index, TokenKind::Dot) {
                return Err(self.error_at(index, "expected `.` between property names"));
            }
            index += 1;
            path.push(self.word_at(index).ok_or_else(|| self.error_at(index, "expected a property name"))?.to_string());
            index += 1;
        }
        Ok(path)
    }

    fn trim_newlines(&self, mut start: usize, mut end: usize) -> (usize, usize) {
        while start < end && self.token_is(start, TokenKind::Newline) { start += 1; }
        while end > start && self.token_is(end - 1, TokenKind::Newline) { end -= 1; }
        (start, end)
    }

    fn starts_with_line_phrase(&self, phrase: &[&str]) -> bool {
        let end = self.find_line_end(self.cursor);
        self.starts_with_phrase(self.cursor, end, phrase)
    }

    fn find_line_end(&self, start: usize) -> usize {
        self.find_boundary(start, self.tokens.len(), Boundary::Line)
    }

    fn find_first_word(&self, start: usize, end: usize, word: &str) -> Option<usize> {
        self.find_first_phrase(start, end, &[word])
    }

    fn find_first_phrase(&self, start: usize, end: usize, phrase: &[&str]) -> Option<usize> {
        let index = self.find_boundary(start, end, Boundary::Phrase(phrase));
        (index < end).then_some(index)
    }

    fn find_first_comma(&self, start: usize, end: usize) -> Option<usize> {
        let index = self.find_boundary(start, end, Boundary::Comma);
        (index < end).then_some(index)
    }

    fn find_first_top_level_kind(&self, start: usize, end: usize, kind: TokenKind) -> Option<usize> {
        let mut depth = 0;
        for index in start..end {
            match &self.tokens[index].kind {
                TokenKind::LeftBracket | TokenKind::LeftParen | TokenKind::LeftBrace => depth += 1,
                TokenKind::RightBracket | TokenKind::RightParen | TokenKind::RightBrace => depth -= 1,
                _ if depth == 0 && self.tokens[index].kind == kind => return Some(index),
                _ => {}
            }
        }
        None
    }

    fn find_last_token(&self, start: usize, end: usize, kind: TokenKind) -> Option<usize> {
        let mut result = None;
        let mut depth = 0;
        for index in start..end {
            match &self.tokens[index].kind {
                TokenKind::LeftBracket | TokenKind::LeftParen | TokenKind::LeftBrace => depth += 1,
                TokenKind::RightBracket | TokenKind::RightParen | TokenKind::RightBrace => depth -= 1,
                _ if depth == 0 && self.tokens[index].kind == kind => result = Some(index),
                _ => {}
            }
        }
        result
    }

    fn find_boundary(&self, start: usize, end: usize, boundary: Boundary<'_>) -> usize {
        let mut depth = 0;
        let mut index = start;
        while index < end {
            match &self.tokens[index].kind {
                TokenKind::LeftBracket | TokenKind::LeftParen | TokenKind::LeftBrace => {
                    depth += 1;
                    index += 1;
                }
                TokenKind::RightBracket | TokenKind::RightParen | TokenKind::RightBrace => {
                    depth -= 1;
                    index += 1;
                }
                TokenKind::Newline if depth == 0 && matches!(boundary, Boundary::Line) => return index,
                TokenKind::End if depth == 0 && matches!(boundary, Boundary::Line) => return index,
                TokenKind::Comma if depth == 0 && matches!(boundary, Boundary::Comma) => return index,
                _ if depth == 0 && boundary.matches(self, index, end) => return index,
                _ => index += 1,
            }
        }
        end
    }

    fn find_repeat_marker(&self, start: usize) -> Option<usize> {
        let end = self.find_line_end(start);
        let mut depth = 0;
        let mut index = start;
        while index + 2 < end {
            match &self.tokens[index].kind {
                TokenKind::LeftBracket | TokenKind::LeftParen | TokenKind::LeftBrace => {
                    depth += 1;
                    index += 1;
                }
                TokenKind::RightBracket | TokenKind::RightParen | TokenKind::RightBrace => {
                    depth -= 1;
                    index += 1;
                }
                _ if depth == 0 && self.word_at(index) == Some("times") && self.token_is(index + 1, TokenKind::Comma) && self.word_at(index + 2) == Some("do") => return Some(index),
                _ => index += 1,
            }
        }
        None
    }

    fn find_last_phrase(&self, start: usize, end: usize, phrase: &[&str]) -> Option<usize> {
        let mut result = None;
        let mut depth = 0;
        let mut index = start;
        while index < end {
            match &self.tokens[index].kind {
                TokenKind::LeftBracket | TokenKind::LeftParen | TokenKind::LeftBrace => {
                    depth += 1;
                    index += 1;
                }
                TokenKind::RightBracket | TokenKind::RightParen | TokenKind::RightBrace => {
                    depth -= 1;
                    index += 1;
                }
                _ if depth == 0 && self.starts_with_phrase(index, end, phrase) => {
                    result = Some(index);
                    index += phrase.len();
                }
                _ => index += 1,
            }
        }
        result
    }

    fn find_last_word_operator(&self, start: usize, end: usize, words: &[&str]) -> Option<usize> {
        let mut result = None;
        let mut depth = 0;
        for index in start..end {
            match &self.tokens[index].kind {
                TokenKind::LeftBracket | TokenKind::LeftParen | TokenKind::LeftBrace => depth += 1,
                TokenKind::RightBracket | TokenKind::RightParen | TokenKind::RightBrace => depth -= 1,
                _ if depth == 0 && self.word_at(index).is_some_and(|word| words.contains(&word)) => result = Some(index),
                _ => {}
            }
        }
        result
    }

    fn starts_with_phrase(&self, start: usize, end: usize, phrase: &[&str]) -> bool {
        start + phrase.len() <= end && phrase.iter().enumerate().all(|(offset, word)| self.word_at(start + offset) == Some(*word))
    }

    fn expect_word_at(&self, index: usize) -> Result<String, BasisError> {
        self.word_at(index).map(str::to_string).ok_or_else(|| self.error_at(index, "expected a property name"))
    }

    fn split_ranges(&self, start: usize, end: usize, separator: TokenKind) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        let mut range_start = start;
        let mut depth = 0;
        for index in start..end {
            match &self.tokens[index].kind {
                TokenKind::LeftBracket | TokenKind::LeftParen | TokenKind::LeftBrace => depth += 1,
                TokenKind::RightBracket | TokenKind::RightParen | TokenKind::RightBrace => depth -= 1,
                _ if depth == 0 && self.tokens[index].kind == separator => {
                    ranges.push((range_start, index));
                    range_start = index + 1;
                }
                _ => {}
            }
        }
        ranges.push((range_start, end));
        ranges
    }

    fn is_wrapped(&self, start: usize, end: usize, open: TokenKind, close: TokenKind) -> bool {
        self.token_is(start, open) && self.token_is(end.saturating_sub(1), close) && self.matching_delimiter(start, end) == Some(end - 1)
    }

    fn matching_delimiter(&self, start: usize, end: usize) -> Option<usize> {
        let mut depth = 0;
        for index in start..end {
            if self.token_is(index, TokenKind::LeftParen) || self.token_is(index, TokenKind::LeftBracket) || self.token_is(index, TokenKind::LeftBrace) {
                depth += 1;
            } else if self.token_is(index, TokenKind::RightParen) || self.token_is(index, TokenKind::RightBracket) || self.token_is(index, TokenKind::RightBrace) {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
        }
        None
    }

    fn tokens_to_text(&self, start: usize, end: usize) -> String {
        (start..end).map(|index| match &self.tokens[index].kind {
            TokenKind::Word(value) => value.clone(),
            TokenKind::Text(value) => value.clone(),
            TokenKind::Number(value) => value.to_string(),
            TokenKind::Comma => ",".to_string(),
            TokenKind::LeftBracket => "[".to_string(),
            TokenKind::RightBracket => "]".to_string(),
            TokenKind::LeftParen => "(".to_string(),
            TokenKind::RightParen => ")".to_string(),
            TokenKind::LeftBrace => "{".to_string(),
            TokenKind::RightBrace => "}".to_string(),
            TokenKind::Colon => ":".to_string(),
            TokenKind::Dot => ".".to_string(),
            TokenKind::Operator(value) => value.clone(),
            TokenKind::Symbol(value) => value.to_string(),
            TokenKind::Newline | TokenKind::End => String::new(),
        }).filter(|value| !value.is_empty()).collect::<Vec<_>>().join(" ")
    }

    fn finish_line(&mut self) -> Result<(), BasisError> {
        if self.consume_newline() || self.at_end() {
            Ok(())
        } else {
            Err(self.error_here("expected the end of the line"))
        }
    }

    fn skip_newlines(&mut self) {
        while self.consume_newline() {}
    }

    fn at_line_end(&self) -> bool {
        matches!(self.current_kind(), TokenKind::Newline | TokenKind::End)
    }

    fn at_end(&self) -> bool {
        matches!(self.current_kind(), TokenKind::End)
    }

    fn current_kind(&self) -> &TokenKind {
        &self.tokens[self.cursor.min(self.tokens.len() - 1)].kind
    }

    fn token_is(&self, index: usize, kind: TokenKind) -> bool {
        self.tokens.get(index).is_some_and(|token| token.kind == kind)
    }

    fn word_at(&self, index: usize) -> Option<&str> {
        match self.tokens.get(index).map(|token| &token.kind) {
            Some(TokenKind::Word(value)) => Some(value.as_str()),
            _ => None,
        }
    }

    fn is_word(&self, word: &str) -> bool {
        self.word_at(self.cursor) == Some(word)
    }

    fn consume_word(&mut self, word: &str) -> bool {
        if self.is_word(word) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn expect_word(&mut self, word: &str) -> Result<(), BasisError> {
        if self.consume_word(word) {
            Ok(())
        } else {
            Err(self.error_here(format!("expected `{word}`")))
        }
    }

    fn expect_any_word(&mut self) -> Result<String, BasisError> {
        let Some(word) = self.word_at(self.cursor).map(str::to_string) else {
            return Err(self.error_here("expected a word"));
        };
        self.cursor += 1;
        Ok(word)
    }

    fn consume_comma(&mut self) -> bool {
        if self.token_is(self.cursor, TokenKind::Comma) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn expect_comma(&mut self) -> Result<(), BasisError> {
        if self.consume_comma() {
            Ok(())
        } else {
            Err(self.error_here("expected `,`"))
        }
    }

    fn is_comma_do(&self) -> bool {
        self.token_is(self.cursor, TokenKind::Comma) && self.word_at(self.cursor + 1) == Some("do")
    }

    fn consume_newline(&mut self) -> bool {
        if self.token_is(self.cursor, TokenKind::Newline) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn error_here(&self, message: impl Into<String>) -> BasisError {
        self.error_at(self.cursor, message)
    }

    fn error_at(&self, index: usize, message: impl Into<String>) -> BasisError {
        let token = self.tokens.get(index).or_else(|| self.tokens.last());
        let line = token.map(|token| token.span.line).unwrap_or(1);
        let column = token.map(|token| token.span.column).unwrap_or(1);
        BasisError::at(line, column, message)
    }
}

enum Boundary<'a> {
    Line,
    Comma,
    Phrase(&'a [&'a str]),
}

fn is_terminal_color(color: &str) -> bool {
    matches!(color.to_ascii_lowercase().as_str(), "black" | "red" | "green" | "yellow" | "blue" | "magenta" | "cyan" | "white" | "gray" | "grey" | "bright black" | "bright red" | "bright green" | "bright yellow" | "bright blue" | "bright magenta" | "bright cyan" | "bright white")
}

impl Boundary<'_> {
    fn matches(&self, parser: &Parser, index: usize, end: usize) -> bool {
        match self {
            Self::Line | Self::Comma => false,
            Self::Phrase(phrase) => parser.starts_with_phrase(index, end, phrase),
        }
    }
}
