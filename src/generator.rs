use crate::ast::ASTNode;
use crate::meta_parser::{OutputExpr, OutputGrammar};

/// コード生成器
/// 出力BNFに基づいてASTから出力コードを生成する
pub struct Generator<'a> {
    grammar: &'a OutputGrammar,
}

impl<'a> Generator<'a> {
    pub fn new(grammar: &'a OutputGrammar) -> Self {
        Generator { grammar }
    }

    /// ASTから出力コードを生成
    pub fn generate(&self, ast: &ASTNode) -> String {
        // 最初の呼び出しはコンテキストなし
        self.generate_rule(&ast.name, ast, "")
    }

    /// 指定したルールに基づいて生成
    /// context: このルールを呼び出した親ルール名
    fn generate_rule(&self, rule_name: &str, ast: &ASTNode, context: &str) -> String {
        if let Some(rule) = self.grammar.rules.get(rule_name) {
            self.generate_expr(&rule.expr, ast, rule_name, context)
        } else {
            // 出力ルールが見つからない場合は、ASTの値をそのまま返す
            if !ast.value.is_empty() {
                ast.value.clone()
            } else {
                // 子ノードを再帰的に処理
                let mut result = String::new();
                for (_, children) in &ast.children {
                    for child in children {
                        result.push_str(&self.generate_rule(&child.name, child, rule_name));
                    }
                }
                result
            }
        }
    }

    /// 式に基づいて生成
    /// current_rule: 現在処理中のルール名
    /// context: このルールを呼び出した親ルール名
    fn generate_expr(&self, expr: &OutputExpr, ast: &ASTNode, current_rule: &str, context: &str) -> String {
        match expr {
            OutputExpr::Literal(lit) => {
                // エスケープシーケンスを処理
                lit.replace("\\n", "\n")
                   .replace("\\t", "\t")
                   .replace("\\r", "\r")
            }

            OutputExpr::RuleRef(name) => {
                // ASTから対応する子ノードを検索
                if let Some(child) = ast.get_child(name) {
                    // 子ルールを呼ぶ時は、現在のルール名をコンテキストとして渡す
                    self.generate_rule(name, child, current_rule)
                } else if &ast.name == name {
                    // 現在のノード自体がそのルールの場合
                    self.generate_rule(name, ast, current_rule)
                } else if !ast.value.is_empty() && ast.children.is_empty() {
                    // 葉ノードの場合のみフォールバック
                    // (例: call_arg := name; で call_arg が name の値を直接持つ場合)
                    self.generate_rule(name, ast, current_rule)
                } else {
                    // 子ノードが存在しない場合は空文字を返す
                    String::new()
                }
            }

            OutputExpr::Sequence(items) => {
                let mut result = String::new();
                for item in items {
                    result.push_str(&self.generate_expr(item, ast, current_rule, context));
                }
                result
            }

            OutputExpr::Optional(inner) => {
                // 対応する子ノードが存在するかチェック
                let inner_result = self.generate_expr(inner, ast, current_rule, context);
                if inner_result.trim().is_empty() {
                    String::new()
                } else {
                    inner_result
                }
            }

            OutputExpr::Join { rule, separator } => {
                // 指定されたルールの全ての子ノードをセパレータで結合
                let children = ast.get_children(rule);
                let parts: Vec<String> = children
                    .iter()
                    .map(|child| self.generate_rule(rule, child, current_rule))
                    .collect();
                // セパレータのエスケープシーケンスを処理
                let sep = separator
                    .replace("\\n", "\n")
                    .replace("\\t", "\t")
                    .replace("\\r", "\r");
                parts.join(&sep)
            }

            OutputExpr::Match { cases, default } => {
                // @valueに基づいてマッチング
                let value = &ast.value;
                for (pattern, replacement) in cases {
                    if value == pattern {
                        return replacement.clone();
                    }
                }
                // デフォルトケース
                if default == "@value" {
                    value.clone()
                } else {
                    default.clone()
                }
            }

            OutputExpr::ContextIf { context_value, then_expr, else_expr } => {
                // コンテキスト（親ルール名）に基づいて条件分岐
                if context == context_value {
                    self.generate_expr(then_expr, ast, current_rule, context)
                } else {
                    self.generate_expr(else_expr, ast, current_rule, context)
                }
            }

            OutputExpr::Choice(alternatives) => {
                // 各選択肢を試して、最初に成功したものを返す
                for alt in alternatives {
                    let result = self.generate_expr(alt, ast, current_rule, context);
                    if !result.is_empty() {
                        return result;
                    }
                }
                String::new()
            }
        }
    }
}
