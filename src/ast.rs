use std::collections::HashMap;

/// 汎用AST ノード
/// 入力BNFでパースした結果を保持する
#[derive(Debug, Clone)]
pub struct ASTNode {
    /// ルール名 (例: "func_decl", "arg")
    pub name: String,

    /// マッチした生テキスト (葉ノードやリテラルの場合)
    pub value: String,

    /// 子ノードマップ
    /// Key: Input BNFで定義された子要素のルール名
    /// Value: マッチしたノードのリスト (`*` や `+` に対応するため Vec)
    pub children: HashMap<String, Vec<ASTNode>>,
}

impl ASTNode {
    pub fn new(name: &str) -> Self {
        ASTNode {
            name: name.to_string(),
            value: String::new(),
            children: HashMap::new(),
        }
    }

    pub fn with_value(name: &str, value: &str) -> Self {
        ASTNode {
            name: name.to_string(),
            value: value.to_string(),
            children: HashMap::new(),
        }
    }

    /// 子ノードを追加
    pub fn add_child(&mut self, child: ASTNode) {
        self.children
            .entry(child.name.clone())
            .or_insert_with(Vec::new)
            .push(child);
    }

    /// 指定したルール名の最初の子を取得
    pub fn get_child(&self, name: &str) -> Option<&ASTNode> {
        self.children.get(name).and_then(|v| v.first())
    }

    /// 指定したルール名の全ての子を取得
    pub fn get_children(&self, name: &str) -> &[ASTNode] {
        self.children.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }
}
