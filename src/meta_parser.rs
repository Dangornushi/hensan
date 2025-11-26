use regex::Regex;
use std::collections::HashMap;

/// 文法式 (入力BNF用)
#[derive(Debug, Clone)]
pub enum GrammarExpr {
    /// 文字列リテラル "..."
    Literal(String),
    /// 正規表現パターン (角括弧で囲まれた部分)
    Pattern(String),
    /// 他のルールへの参照
    RuleRef(String),
    /// 連続 (A B C)
    Sequence(Vec<GrammarExpr>),
    /// 選択 (A | B)
    Choice(Vec<GrammarExpr>),
    /// 0回以上 (A*)
    ZeroOrMore(Box<GrammarExpr>),
    /// 1回以上 (A+)
    OneOrMore(Box<GrammarExpr>),
    /// 省略可能 (A?)
    Optional(Box<GrammarExpr>),
    /// グループ化 (...)
    Group(Box<GrammarExpr>),
    /// インデント増加 (INDENT)
    Indent,
    /// インデント減少 (DEDENT)
    Dedent,
    /// 改行 (NEWLINE)
    Newline,
    /// 現在のインデントレベルと一致 (SAME_INDENT)
    SameIndent,
}

/// 出力BNF用の式
#[derive(Debug, Clone)]
pub enum OutputExpr {
    /// 文字列リテラル
    Literal(String),
    /// ルール参照
    RuleRef(String),
    /// 連続
    Sequence(Vec<OutputExpr>),
    /// 省略可能
    Optional(Box<OutputExpr>),
    /// Join構文: rule join "separator"
    Join { rule: String, separator: String },
    /// Match構文
    Match {
        cases: Vec<(String, String)>,
        default: String,
    },
    /// コンテキスト条件分岐: if @context == "rule_name" then expr else expr
    ContextIf {
        context_value: String,
        then_expr: Box<OutputExpr>,
        else_expr: Box<OutputExpr>,
    },
}

/// 入力BNFのルール
#[derive(Debug, Clone)]
pub struct InputRule {
    pub name: String,
    pub expr: GrammarExpr,
}

/// 出力BNFのルール
#[derive(Debug, Clone)]
pub struct OutputRule {
    pub name: String,
    pub expr: OutputExpr,
}

/// 入力BNF全体
#[derive(Debug)]
pub struct InputGrammar {
    pub rules: HashMap<String, InputRule>,
    pub start_rule: String,
}

/// 出力BNF全体
#[derive(Debug)]
pub struct OutputGrammar {
    pub rules: HashMap<String, OutputRule>,
}

/// BNFパーサー
pub struct MetaParser {
    input: String,
    pos: usize,
}

impl MetaParser {
    pub fn new(input: &str) -> Self {
        MetaParser {
            input: input.to_string(),
            pos: 0,
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // 空白スキップ
            while self.pos < self.input.len() {
                let ch = self.input[self.pos..].chars().next().unwrap();
                if ch.is_whitespace() {
                    self.pos += ch.len_utf8();
                } else {
                    break;
                }
            }
            // コメントスキップ
            if self.input[self.pos..].starts_with("//") {
                while self.pos < self.input.len() {
                    let ch = self.input[self.pos..].chars().next().unwrap();
                    self.pos += ch.len_utf8();
                    if ch == '\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn consume_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn expect_char(&mut self, expected: char) {
        let ch = self.consume_char().expect("Unexpected end of input");
        assert_eq!(ch, expected, "Expected '{}', got '{}'", expected, ch);
    }

    fn parse_identifier(&mut self) -> String {
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch.is_alphanumeric() || ch == '_' {
                self.consume_char();
            } else {
                break;
            }
        }
        self.input[start..self.pos].to_string()
    }

    fn parse_string_literal(&mut self) -> String {
        self.expect_char('"');
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch == '"' {
                break;
            }
            self.consume_char();
        }
        let result = self.input[start..self.pos].to_string();
        self.expect_char('"');
        result
    }

    fn parse_pattern(&mut self) -> String {
        self.expect_char('[');
        let start = self.pos;
        let mut depth = 1;
        while depth > 0 {
            let ch = self.consume_char().expect("Unclosed pattern");
            if ch == '[' {
                depth += 1;
            } else if ch == ']' {
                depth -= 1;
            }
        }
        self.input[start..self.pos - 1].to_string()
    }

    /// 入力BNFをパース
    pub fn parse_input_grammar(&mut self) -> InputGrammar {
        let mut rules = HashMap::new();
        let mut start_rule = String::new();

        while self.pos < self.input.len() {
            self.skip_whitespace_and_comments();
            if self.pos >= self.input.len() {
                break;
            }

            let name = self.parse_identifier();
            if name.is_empty() {
                break;
            }

            if start_rule.is_empty() {
                start_rule = name.clone();
            }

            self.skip_whitespace_and_comments();
            // := を消費
            assert!(
                self.input[self.pos..].starts_with(":="),
                "Expected ':=' after rule name"
            );
            self.pos += 2;

            self.skip_whitespace_and_comments();
            let expr = self.parse_input_expr();

            self.skip_whitespace_and_comments();
            self.expect_char(';');

            rules.insert(name.clone(), InputRule { name, expr });
        }

        InputGrammar { rules, start_rule }
    }

    fn parse_input_expr(&mut self) -> GrammarExpr {
        let mut choices = vec![self.parse_input_sequence()];

        loop {
            self.skip_whitespace_and_comments();
            if self.peek_char() == Some('|') {
                self.consume_char();
                self.skip_whitespace_and_comments();
                choices.push(self.parse_input_sequence());
            } else {
                break;
            }
        }

        if choices.len() == 1 {
            choices.pop().unwrap()
        } else {
            GrammarExpr::Choice(choices)
        }
    }

    fn parse_input_sequence(&mut self) -> GrammarExpr {
        let mut items = Vec::new();

        loop {
            self.skip_whitespace_and_comments();
            if let Some(item) = self.parse_input_atom() {
                items.push(item);
            } else {
                break;
            }
        }

        if items.len() == 1 {
            items.pop().unwrap()
        } else {
            GrammarExpr::Sequence(items)
        }
    }

    fn parse_input_atom(&mut self) -> Option<GrammarExpr> {
        self.skip_whitespace_and_comments();

        let ch = self.peek_char()?;

        let base = match ch {
            '"' => {
                let lit = self.parse_string_literal();
                // 正規表現メタ文字を含む場合はパターンとして扱う
                if lit.starts_with('[') || lit.contains('+') || lit.contains('*') || lit.contains('\\') {
                    GrammarExpr::Pattern(lit)
                } else {
                    GrammarExpr::Literal(lit)
                }
            }
            '[' => {
                let pattern = self.parse_pattern();
                GrammarExpr::Pattern(pattern)
            }
            '(' => {
                self.consume_char();
                self.skip_whitespace_and_comments();
                let inner = self.parse_input_expr();
                self.skip_whitespace_and_comments();
                self.expect_char(')');
                GrammarExpr::Group(Box::new(inner))
            }
            _ if ch.is_alphabetic() || ch == '_' => {
                let name = self.parse_identifier();
                // 特殊トークンをチェック
                match name.as_str() {
                    "INDENT" => GrammarExpr::Indent,
                    "DEDENT" => GrammarExpr::Dedent,
                    "NEWLINE" => GrammarExpr::Newline,
                    _ => GrammarExpr::RuleRef(name),
                }
            }
            _ => return None,
        };

        // 後置演算子をチェック
        self.skip_whitespace_and_comments();
        match self.peek_char() {
            Some('*') => {
                self.consume_char();
                Some(GrammarExpr::ZeroOrMore(Box::new(base)))
            }
            Some('+') => {
                self.consume_char();
                Some(GrammarExpr::OneOrMore(Box::new(base)))
            }
            Some('?') => {
                self.consume_char();
                Some(GrammarExpr::Optional(Box::new(base)))
            }
            _ => Some(base),
        }
    }

    /// 出力BNFをパース
    pub fn parse_output_grammar(&mut self) -> OutputGrammar {
        let mut rules = HashMap::new();

        while self.pos < self.input.len() {
            self.skip_whitespace_and_comments();
            if self.pos >= self.input.len() {
                break;
            }

            let name = self.parse_identifier();
            if name.is_empty() {
                break;
            }

            self.skip_whitespace_and_comments();
            assert!(
                self.input[self.pos..].starts_with(":="),
                "Expected ':=' after rule name"
            );
            self.pos += 2;

            self.skip_whitespace_and_comments();
            let expr = self.parse_output_expr();

            self.skip_whitespace_and_comments();
            self.expect_char(';');

            rules.insert(name.clone(), OutputRule { name, expr });
        }

        OutputGrammar { rules }
    }

    fn parse_output_expr(&mut self) -> OutputExpr {
        self.skip_whitespace_and_comments();

        // match構文のチェック (matchの後が識別子文字でないことを確認)
        if self.input[self.pos..].starts_with("match") {
            let after_match = self.input[self.pos + 5..].chars().next();
            if after_match.map_or(true, |ch| !ch.is_alphanumeric() && ch != '_') {
                return self.parse_match_expr();
            }
        }

        // if @context構文のチェック
        if self.input[self.pos..].starts_with("if") {
            let after_if = self.input[self.pos + 2..].chars().next();
            if after_if.map_or(true, |ch| !ch.is_alphanumeric() && ch != '_') {
                return self.parse_context_if_expr();
            }
        }

        let mut items = Vec::new();

        loop {
            self.skip_whitespace_and_comments();
            if let Some(item) = self.parse_output_atom() {
                // join構文のチェック
                self.skip_whitespace_and_comments();
                if self.input[self.pos..].starts_with("join") {
                    self.pos += 4;
                    self.skip_whitespace_and_comments();
                    let separator = self.parse_string_literal();
                    if let OutputExpr::RuleRef(rule) = item {
                        items.push(OutputExpr::Join { rule, separator });
                    } else {
                        panic!("join must follow a rule reference");
                    }
                } else {
                    items.push(item);
                }
            } else {
                break;
            }
        }

        if items.len() == 1 {
            items.pop().unwrap()
        } else {
            OutputExpr::Sequence(items)
        }
    }

    fn parse_output_atom(&mut self) -> Option<OutputExpr> {
        self.skip_whitespace_and_comments();

        let ch = self.peek_char()?;

        match ch {
            '"' => {
                let lit = self.parse_string_literal();
                Some(OutputExpr::Literal(lit))
            }
            '(' => {
                self.consume_char();
                self.skip_whitespace_and_comments();
                let inner = self.parse_output_expr();
                self.skip_whitespace_and_comments();
                self.expect_char(')');

                // 後置演算子
                self.skip_whitespace_and_comments();
                if self.peek_char() == Some('?') {
                    self.consume_char();
                    Some(OutputExpr::Optional(Box::new(inner)))
                } else {
                    Some(inner)
                }
            }
            _ if ch.is_alphabetic() || ch == '_' => {
                let name = self.parse_identifier();
                // 後置演算子
                self.skip_whitespace_and_comments();
                if self.peek_char() == Some('?') {
                    self.consume_char();
                    Some(OutputExpr::Optional(Box::new(OutputExpr::RuleRef(name))))
                } else {
                    Some(OutputExpr::RuleRef(name))
                }
            }
            _ => None,
        }
    }

    fn parse_match_expr(&mut self) -> OutputExpr {
        // "match" を消費
        self.pos += 5;
        self.skip_whitespace_and_comments();

        // "@value" を期待
        assert!(
            self.input[self.pos..].starts_with("@value"),
            "Expected @value after match"
        );
        self.pos += 6;

        self.skip_whitespace_and_comments();
        self.expect_char('{');

        let mut cases = Vec::new();
        let mut default = String::new();

        loop {
            self.skip_whitespace_and_comments();

            if self.peek_char() == Some('}') {
                self.consume_char();
                break;
            }

            // パターン部分
            if self.peek_char() == Some('_') {
                // デフォルトケース
                self.consume_char();
                self.skip_whitespace_and_comments();
                assert!(
                    self.input[self.pos..].starts_with("=>"),
                    "Expected '=>' in match"
                );
                self.pos += 2;
                self.skip_whitespace_and_comments();

                if self.input[self.pos..].starts_with("@value") {
                    self.pos += 6;
                    default = "@value".to_string();
                } else {
                    default = self.parse_string_literal();
                }
            } else if self.peek_char() == Some('"') {
                let pattern = self.parse_string_literal();
                self.skip_whitespace_and_comments();
                assert!(
                    self.input[self.pos..].starts_with("=>"),
                    "Expected '=>' in match"
                );
                self.pos += 2;
                self.skip_whitespace_and_comments();
                let replacement = self.parse_string_literal();
                cases.push((pattern, replacement));
            } else {
                break;
            }

            // カンマをスキップ (あれば)
            self.skip_whitespace_and_comments();
            if self.peek_char() == Some(',') {
                self.consume_char();
            }
        }

        OutputExpr::Match { cases, default }
    }

    /// if @context == "value" then expr else expr をパース
    fn parse_context_if_expr(&mut self) -> OutputExpr {
        // "if" を消費
        self.pos += 2;
        self.skip_whitespace_and_comments();

        // "@context" を期待
        assert!(
            self.input[self.pos..].starts_with("@context"),
            "Expected @context after if"
        );
        self.pos += 8;

        self.skip_whitespace_and_comments();

        // "==" を期待
        assert!(
            self.input[self.pos..].starts_with("=="),
            "Expected '==' after @context"
        );
        self.pos += 2;

        self.skip_whitespace_and_comments();

        // コンテキスト値（文字列リテラル）
        let context_value = self.parse_string_literal();

        self.skip_whitespace_and_comments();

        // "then" を期待
        assert!(
            self.input[self.pos..].starts_with("then"),
            "Expected 'then' after context value"
        );
        self.pos += 4;

        self.skip_whitespace_and_comments();

        // then式をパース（括弧で囲まれた式、または単一のアトム）
        let then_expr = if self.peek_char() == Some('(') {
            self.consume_char();
            self.skip_whitespace_and_comments();
            let inner = self.parse_output_expr();
            self.skip_whitespace_and_comments();
            self.expect_char(')');
            inner
        } else {
            self.parse_output_atom().expect("Expected expression after 'then'")
        };

        self.skip_whitespace_and_comments();

        // "else" を期待
        assert!(
            self.input[self.pos..].starts_with("else"),
            "Expected 'else' after then expression"
        );
        self.pos += 4;

        self.skip_whitespace_and_comments();

        // else式をパース（括弧で囲まれた式、または単一のアトム）
        let else_expr = if self.peek_char() == Some('(') {
            self.consume_char();
            self.skip_whitespace_and_comments();
            let inner = self.parse_output_expr();
            self.skip_whitespace_and_comments();
            self.expect_char(')');
            inner
        } else {
            self.parse_output_atom().expect("Expected expression after 'else'")
        };

        OutputExpr::ContextIf {
            context_value,
            then_expr: Box::new(then_expr),
            else_expr: Box::new(else_expr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_input_grammar() {
        let input = r#"
            func_decl := ret_type name "(" args? ")" ";";
            args      := arg ("," arg)*;
            arg       := type name;
            ret_type  := "void" | "int";
            type      := "int" | "float";
            name      := "[a-zA-Z_]+";
        "#;

        let mut parser = MetaParser::new(input);
        let grammar = parser.parse_input_grammar();

        assert!(grammar.rules.contains_key("func_decl"));
        assert!(grammar.rules.contains_key("args"));
        assert!(grammar.rules.contains_key("arg"));
    }
}
