use regex::Regex;
use std::collections::HashMap;
use std::fmt;

use crate::ast::ASTNode;
use crate::meta_parser::{GrammarExpr, InputGrammar};

/// パースエラー情報
#[derive(Debug, Clone)]
pub struct ParseError {
    /// エラー発生位置 (バイトオフセット)
    pub position: usize,
    /// 行番号 (1-indexed)
    pub line: usize,
    /// 列番号 (1-indexed)
    pub column: usize,
    /// 期待されたもの
    pub expected: Vec<String>,
    /// 実際に見つかったもの
    pub found: String,
    /// パース試行中だったルール
    pub context_rule: String,
    /// ソースコードの該当行
    pub source_line: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Parse error at line {}, column {}:", self.line, self.column)?;
        writeln!(f)?;

        // 行番号付きでソース行を表示
        let line_num_width = self.line.to_string().len();
        writeln!(f, " {:>width$} | {}", self.line, self.source_line, width = line_num_width)?;

        // エラー位置を示す矢印
        let arrow_padding = " ".repeat(line_num_width + 3 + self.column - 1);
        writeln!(f, "{}^", arrow_padding)?;
        writeln!(f)?;

        // 期待されたものと実際に見つかったもの
        if !self.expected.is_empty() {
            writeln!(f, "Expected: {}", self.expected.join(" or "))?;
        }
        writeln!(f, "Found: '{}'", self.found)?;
        writeln!(f, "While parsing: {}", self.context_rule)?;

        Ok(())
    }
}

/// パース結果
pub type ParseResult = Result<ASTNode, ParseError>;

/// ソースコードパーサー
/// 入力BNFに基づいてソースコードをパースし、ASTを構築する
pub struct Parser<'a> {
    grammar: &'a InputGrammar,
    input: String,
    pos: usize,
    /// 正規表現のキャッシュ
    regex_cache: HashMap<String, Regex>,
    /// 最も遠くまで進んだ位置 (エラー報告用)
    furthest_pos: usize,
    /// その位置で期待されていたもの
    furthest_expected: Vec<String>,
    /// その位置でパース中だったルール
    furthest_rule: String,
    /// インデントスタック (インデントレベルを追跡)
    indent_stack: Vec<usize>,
    /// 保留中のDEDENTトークン数
    pending_dedents: usize,
    /// 行の先頭かどうか
    at_line_start: bool,
    /// 現在の行のインデントレベル (スペース数)
    current_line_indent: usize,
}

impl<'a> Parser<'a> {
    pub fn new(grammar: &'a InputGrammar, input: &str) -> Self {
        Parser {
            grammar,
            input: input.to_string(),
            pos: 0,
            regex_cache: HashMap::new(),
            furthest_pos: 0,
            furthest_expected: Vec::new(),
            furthest_rule: String::new(),
            indent_stack: vec![0], // 初期インデントレベルは0
            pending_dedents: 0,
            at_line_start: true,
            current_line_indent: 0,
        }
    }

    /// ソースコードをパースしてASTを返す
    pub fn parse(&mut self) -> ParseResult {
        // 最初の行のインデントを計算
        self.update_line_indent();

        let start_rule = self.grammar.start_rule.clone();
        let result = self.parse_rule(&start_rule);
        self.skip_whitespace_no_newline();

        match result {
            Some(ast) => {
                // 入力を全て消費したかチェック
                if self.pos < self.input.len() {
                    self.record_error("end of input", &start_rule);
                    Err(self.build_error())
                } else {
                    Ok(ast)
                }
            }
            None => Err(self.build_error()),
        }
    }

    /// 現在行のインデントレベルを更新
    fn update_line_indent(&mut self) {
        if !self.at_line_start {
            return;
        }

        let mut indent = 0;
        let mut temp_pos = self.pos;

        while temp_pos < self.input.len() {
            let ch = self.input[temp_pos..].chars().next().unwrap();
            match ch {
                ' ' => {
                    indent += 1;
                    temp_pos += 1;
                }
                '\t' => {
                    // タブは8スペースとして扱う (Python準拠)
                    indent = (indent / 8 + 1) * 8;
                    temp_pos += 1;
                }
                _ => break,
            }
        }

        self.current_line_indent = indent;
    }

    /// エラー情報を記録
    fn record_error(&mut self, expected: &str, context_rule: &str) {
        if self.pos > self.furthest_pos {
            self.furthest_pos = self.pos;
            self.furthest_expected.clear();
            self.furthest_expected.push(expected.to_string());
            self.furthest_rule = context_rule.to_string();
        } else if self.pos == self.furthest_pos {
            let exp = expected.to_string();
            if !self.furthest_expected.contains(&exp) {
                self.furthest_expected.push(exp);
            }
        }
    }

    /// エラー構造体を構築
    fn build_error(&self) -> ParseError {
        let (line, column) = self.pos_to_line_col(self.furthest_pos);
        let source_line = self.get_source_line(line);
        let found = self.get_found_text(self.furthest_pos);

        ParseError {
            position: self.furthest_pos,
            line,
            column,
            expected: self.furthest_expected.clone(),
            found,
            context_rule: self.furthest_rule.clone(),
            source_line,
        }
    }

    /// バイト位置から行番号と列番号を計算
    fn pos_to_line_col(&self, pos: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;

        for (i, ch) in self.input.chars().enumerate() {
            if i >= pos {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }

        (line, col)
    }

    /// 指定行のソースコードを取得
    fn get_source_line(&self, line_num: usize) -> String {
        self.input
            .lines()
            .nth(line_num - 1)
            .unwrap_or("")
            .to_string()
    }

    /// エラー位置で見つかったテキストを取得
    fn get_found_text(&self, pos: usize) -> String {
        let remaining = &self.input[pos..];
        if remaining.is_empty() {
            "end of input".to_string()
        } else {
            // 最大20文字まで表示
            let preview: String = remaining.chars().take(20).collect();
            if remaining.len() > 20 {
                format!("{}...", preview)
            } else {
                preview
            }
        }
    }

    /// 改行以外の空白をスキップ
    fn skip_whitespace_no_newline(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.input[self.pos..].chars().next().unwrap();
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    /// 空白をスキップ (インデントトラッキングなし)
    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.input[self.pos..].chars().next().unwrap();
            if ch.is_whitespace() {
                if ch == '\n' {
                    self.at_line_start = true;
                }
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
        if self.at_line_start {
            self.update_line_indent();
        }
    }

    fn remaining(&self) -> &str {
        &self.input[self.pos..]
    }

    /// 指定したルールをパース
    fn parse_rule(&mut self, rule_name: &str) -> Option<ASTNode> {
        let rule = self.grammar.rules.get(rule_name)?;
        let expr = rule.expr.clone();

        let start_pos = self.pos;
        let result = self.parse_expr(&expr, rule_name);

        if let Some(mut node) = result {
            // ルール名で葉ノードの値を設定
            if node.children.is_empty() && node.value.is_empty() {
                node.value = self.input[start_pos..self.pos].to_string();
            }
            node.name = rule_name.to_string();
            Some(node)
        } else {
            self.pos = start_pos;
            None
        }
    }

    /// 式をパース
    fn parse_expr(&mut self, expr: &GrammarExpr, context_rule: &str) -> Option<ASTNode> {
        match expr {
            GrammarExpr::Literal(lit) => self.parse_literal(lit, context_rule),
            GrammarExpr::Pattern(pattern) => self.parse_pattern(pattern, context_rule),
            GrammarExpr::RuleRef(name) => self.parse_rule(name),
            GrammarExpr::Sequence(items) => self.parse_sequence(items, context_rule),
            GrammarExpr::Choice(choices) => self.parse_choice(choices, context_rule),
            GrammarExpr::ZeroOrMore(inner) => self.parse_zero_or_more(inner, context_rule),
            GrammarExpr::OneOrMore(inner) => self.parse_one_or_more(inner, context_rule),
            GrammarExpr::Optional(inner) => self.parse_optional(inner, context_rule),
            GrammarExpr::Group(inner) => {
                // グループの結果は内部ノードとして返す (子要素が展開されるように)
                let result = self.parse_expr(inner, context_rule)?;
                let mut group_node = ASTNode::new("_group");
                // 子要素をコピー
                for (name, children) in result.children {
                    for c in children {
                        group_node.children.entry(name.clone()).or_default().push(c);
                    }
                }
                Some(group_node)
            }
            GrammarExpr::Indent => self.parse_indent(context_rule),
            GrammarExpr::Dedent => self.parse_dedent(context_rule),
            GrammarExpr::Newline => self.parse_newline(context_rule),
            GrammarExpr::SameIndent => self.parse_same_indent(context_rule),
        }
    }

    /// INDENT トークンをパース
    fn parse_indent(&mut self, context_rule: &str) -> Option<ASTNode> {
        // 保留中のDEDENTがあればINDENTは失敗
        if self.pending_dedents > 0 {
            self.record_error("INDENT", context_rule);
            return None;
        }

        let current_indent = *self.indent_stack.last().unwrap_or(&0);

        if self.current_line_indent > current_indent {
            // インデントが増加した
            self.indent_stack.push(self.current_line_indent);
            // インデント分のスペースを消費
            self.skip_whitespace_no_newline();
            self.at_line_start = false;
            Some(ASTNode::with_value("_indent", ""))
        } else {
            self.record_error("INDENT", context_rule);
            None
        }
    }

    /// DEDENT トークンをパース
    fn parse_dedent(&mut self, context_rule: &str) -> Option<ASTNode> {
        // 保留中のDEDENTがあれば消費（スタックもpop）
        if self.pending_dedents > 0 {
            self.pending_dedents -= 1;
            self.indent_stack.pop();
            return Some(ASTNode::with_value("_dedent", ""));
        }

        let current_indent = *self.indent_stack.last().unwrap_or(&0);

        if self.current_line_indent < current_indent {
            // インデントが減少した - 1レベルだけpop
            self.indent_stack.pop();

            // さらにDEDENTが必要かチェック（pending_dedentsに記録）
            let new_indent = *self.indent_stack.last().unwrap_or(&0);
            if self.current_line_indent < new_indent {
                // まだDEDENTが必要なレベル数を数える
                let mut extra_dedents = 0;
                let mut test_stack = self.indent_stack.clone();
                while test_stack.len() > 1 {
                    let level = *test_stack.last().unwrap();
                    if self.current_line_indent >= level {
                        break;
                    }
                    test_stack.pop();
                    extra_dedents += 1;
                }
                self.pending_dedents = extra_dedents;
            }

            Some(ASTNode::with_value("_dedent", ""))
        } else {
            self.record_error("DEDENT", context_rule);
            None
        }
    }

    /// NEWLINE トークンをパース（複数の空白行もスキップ）
    fn parse_newline(&mut self, context_rule: &str) -> Option<ASTNode> {
        self.skip_whitespace_no_newline();

        // 最初の改行は必須
        let has_newline = if self.remaining().starts_with('\n') {
            self.pos += 1;
            true
        } else if self.remaining().starts_with("\r\n") {
            self.pos += 2;
            true
        } else {
            false
        };

        if !has_newline {
            self.record_error("NEWLINE", context_rule);
            return None;
        }

        // 追加の空白行をスキップ（空白のみの行 + 改行）
        loop {
            let line_start = self.pos;
            self.skip_whitespace_no_newline();

            if self.remaining().starts_with('\n') {
                self.pos += 1;
            } else if self.remaining().starts_with("\r\n") {
                self.pos += 2;
            } else {
                // 改行がない = コンテンツがある行に到達
                self.pos = line_start;
                break;
            }
        }

        self.at_line_start = true;
        self.update_line_indent();
        Some(ASTNode::with_value("_newline", "\n"))
    }

    /// SAME_INDENT トークンをパース（現在のインデントレベルと一致）
    fn parse_same_indent(&mut self, context_rule: &str) -> Option<ASTNode> {
        let current_indent = *self.indent_stack.last().unwrap_or(&0);

        if self.current_line_indent == current_indent {
            // インデントレベルが一致
            self.skip_whitespace_no_newline();
            self.at_line_start = false;
            Some(ASTNode::with_value("_same_indent", ""))
        } else {
            self.record_error("SAME_INDENT", context_rule);
            None
        }
    }

    fn parse_literal(&mut self, lit: &str, context_rule: &str) -> Option<ASTNode> {
        self.skip_whitespace_no_newline();
        if self.remaining().starts_with(lit) {
            self.pos += lit.len();
            self.at_line_start = false;
            Some(ASTNode::with_value("_literal", lit))
        } else {
            self.record_error(&format!("\"{}\"", lit), context_rule);
            None
        }
    }

    fn parse_pattern(&mut self, pattern: &str, context_rule: &str) -> Option<ASTNode> {
        self.skip_whitespace_no_newline();

        // 正規表現をキャッシュから取得または作成
        let regex_pattern = format!("^{}", pattern);
        let regex = if let Some(r) = self.regex_cache.get(&regex_pattern) {
            r.clone()
        } else {
            let r = Regex::new(&regex_pattern).expect("Invalid regex pattern");
            self.regex_cache.insert(regex_pattern.clone(), r.clone());
            r
        };

        if let Some(m) = regex.find(self.remaining()) {
            let matched = m.as_str().to_string();
            self.pos += matched.len();
            self.at_line_start = false;
            Some(ASTNode::with_value("_pattern", &matched))
        } else {
            self.record_error(&format!("pattern /{}/", pattern), context_rule);
            None
        }
    }

    fn parse_sequence(&mut self, items: &[GrammarExpr], context_rule: &str) -> Option<ASTNode> {
        let start_pos = self.pos;
        let start_indent_stack = self.indent_stack.clone();
        let start_pending_dedents = self.pending_dedents;
        let start_at_line_start = self.at_line_start;
        let start_current_line_indent = self.current_line_indent;

        let mut node = ASTNode::new(context_rule);

        for item in items {
            if let Some(child) = self.parse_expr(item, context_rule) {
                // リテラルや内部ノード以外は子ノードとして追加
                if !child.name.starts_with('_') {
                    node.add_child(child);
                } else if child.name != "_literal" && child.name != "_pattern"
                    && child.name != "_optional_empty" && child.name != "_indent"
                    && child.name != "_dedent" && child.name != "_newline" {
                    // 内部ノード (_repeat など) の子を展開
                    for (name, children) in child.children {
                        for c in children {
                            node.children.entry(name.clone()).or_default().push(c);
                        }
                    }
                }
            } else {
                // パース失敗、バックトラック
                self.pos = start_pos;
                self.indent_stack = start_indent_stack;
                self.pending_dedents = start_pending_dedents;
                self.at_line_start = start_at_line_start;
                self.current_line_indent = start_current_line_indent;
                return None;
            }
        }

        Some(node)
    }

    fn parse_choice(&mut self, choices: &[GrammarExpr], context_rule: &str) -> Option<ASTNode> {
        let start_pos = self.pos;
        let start_indent_stack = self.indent_stack.clone();
        let start_pending_dedents = self.pending_dedents;
        let start_at_line_start = self.at_line_start;
        let start_current_line_indent = self.current_line_indent;

        for choice in choices {
            if let Some(child) = self.parse_expr(choice, context_rule) {
                // 子がすでに同じコンテキストルール名を持つ場合はそのまま返す
                // (Sequenceは既にcontext_ruleの名前で作成されている)
                if child.name == context_rule {
                    return Some(child);
                }
                // 選択結果を子ノードとして保持するラッパーノードを作成
                let mut node = ASTNode::new(context_rule);
                if !child.name.starts_with('_') {
                    node.add_child(child);
                } else {
                    // 内部ノードの場合は子を展開
                    for (name, children) in child.children {
                        for c in children {
                            node.children.entry(name.clone()).or_default().push(c);
                        }
                    }
                }
                return Some(node);
            }
            // バックトラック
            self.pos = start_pos;
            self.indent_stack = start_indent_stack.clone();
            self.pending_dedents = start_pending_dedents;
            self.at_line_start = start_at_line_start;
            self.current_line_indent = start_current_line_indent;
        }

        None
    }

    fn parse_zero_or_more(
        &mut self,
        inner: &GrammarExpr,
        context_rule: &str,
    ) -> Option<ASTNode> {
        let mut node = ASTNode::new("_repeat");

        loop {
            let start_pos = self.pos;
            let start_indent_stack = self.indent_stack.clone();
            let start_pending_dedents = self.pending_dedents;
            let start_at_line_start = self.at_line_start;
            let start_current_line_indent = self.current_line_indent;

            if let Some(child) = self.parse_expr(inner, context_rule) {
                if !child.name.starts_with('_') {
                    node.add_child(child);
                } else if child.name != "_indent" && child.name != "_dedent" && child.name != "_newline" {
                    // グループ内の子ノードを展開
                    for (name, children) in child.children {
                        for c in children {
                            node.children.entry(name.clone()).or_default().push(c);
                        }
                    }
                }
            } else {
                self.pos = start_pos;
                self.indent_stack = start_indent_stack;
                self.pending_dedents = start_pending_dedents;
                self.at_line_start = start_at_line_start;
                self.current_line_indent = start_current_line_indent;
                break;
            }
        }

        Some(node)
    }

    fn parse_one_or_more(
        &mut self,
        inner: &GrammarExpr,
        context_rule: &str,
    ) -> Option<ASTNode> {
        // 最初の1回は必須
        let first = self.parse_expr(inner, context_rule)?;

        let mut node = ASTNode::new("_repeat");
        if !first.name.starts_with('_') {
            node.add_child(first);
        } else if first.name != "_indent" && first.name != "_dedent" && first.name != "_newline" {
            for (name, children) in first.children {
                for c in children {
                    node.children.entry(name.clone()).or_default().push(c);
                }
            }
        }

        // 残りは0回以上
        loop {
            let loop_start = self.pos;
            let start_indent_stack = self.indent_stack.clone();
            let start_pending_dedents = self.pending_dedents;
            let start_at_line_start = self.at_line_start;
            let start_current_line_indent = self.current_line_indent;

            if let Some(child) = self.parse_expr(inner, context_rule) {
                if !child.name.starts_with('_') {
                    node.add_child(child);
                } else if child.name != "_indent" && child.name != "_dedent" && child.name != "_newline" {
                    for (name, children) in child.children {
                        for c in children {
                            node.children.entry(name.clone()).or_default().push(c);
                        }
                    }
                }
            } else {
                self.pos = loop_start;
                self.indent_stack = start_indent_stack;
                self.pending_dedents = start_pending_dedents;
                self.at_line_start = start_at_line_start;
                self.current_line_indent = start_current_line_indent;
                break;
            }
        }

        Some(node)
    }

    fn parse_optional(&mut self, inner: &GrammarExpr, context_rule: &str) -> Option<ASTNode> {
        let start_pos = self.pos;
        let start_indent_stack = self.indent_stack.clone();
        let start_pending_dedents = self.pending_dedents;
        let start_at_line_start = self.at_line_start;
        let start_current_line_indent = self.current_line_indent;

        if let Some(child) = self.parse_expr(inner, context_rule) {
            Some(child)
        } else {
            self.pos = start_pos;
            self.indent_stack = start_indent_stack;
            self.pending_dedents = start_pending_dedents;
            self.at_line_start = start_at_line_start;
            self.current_line_indent = start_current_line_indent;
            // 空のノードを返す (optionalなのでOK)
            Some(ASTNode::new("_optional_empty"))
        }
    }
}
