mod ast;
mod generator;
mod meta_parser;
mod parser;

use std::env;
use std::fs;
use std::path::Path;
use std::process;

use generator::Generator;
use meta_parser::MetaParser;
use parser::Parser;

const GRAMMAR_DIR: &str = "Grammar";
const DEFAULT_INPUT_BNF: &str = "input.bnf";
const DEFAULT_OUTPUT_BNF: &str = "output.bnf";

// デフォルトの入力BNF
const DEFAULT_INPUT_GRAMMAR: &str = r#"func_decl := ret_type name "(" args? ")" ";";
args      := arg ("," arg)*;
arg       := type name;
ret_type  := "void" | "int";
type      := "int" | "float";
name      := "[a-zA-Z_]+";
"#;

// デフォルトの出力BNF
const DEFAULT_OUTPUT_GRAMMAR: &str = r#"// nameとret_typeの位置が移動している
func_decl := "fn " name "(" args? ")" " -> " ret_type ";";

// リストはカンマ区切りで展開
args      := arg join ", ";

// typeとnameの順序が逆転
arg       := name ": " type;

// 型名の変換
ret_type  := match @value {
    "void" => "()",
    "int"  => "i32",
    _ => @value
};
type      := match @value {
    "int" => "i32",
    "float" => "f64",
    _ => @value
};
"#;

fn ensure_grammar_files() {
    let grammar_dir = Path::new(GRAMMAR_DIR);

    // Grammarディレクトリがなければ作成
    if !grammar_dir.exists() {
        fs::create_dir_all(grammar_dir).unwrap_or_else(|e| {
            eprintln!("Error creating Grammar directory: {}", e);
            process::exit(1);
        });
        eprintln!("Created {} directory", GRAMMAR_DIR);
    }

    // input.bnf がなければ作成
    let input_path = grammar_dir.join(DEFAULT_INPUT_BNF);
    if !input_path.exists() {
        fs::write(&input_path, DEFAULT_INPUT_GRAMMAR).unwrap_or_else(|e| {
            eprintln!("Error creating {}: {}", input_path.display(), e);
            process::exit(1);
        });
        eprintln!("Created {}", input_path.display());
    }

    // output.bnf がなければ作成
    let output_path = grammar_dir.join(DEFAULT_OUTPUT_BNF);
    if !output_path.exists() {
        fs::write(&output_path, DEFAULT_OUTPUT_GRAMMAR).unwrap_or_else(|e| {
            eprintln!("Error creating {}: {}", output_path.display(), e);
            process::exit(1);
        });
        eprintln!("Created {}", output_path.display());
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // 使用法の表示
    if args.len() < 2 {
        eprintln!("Usage: {} <source> [input.bnf] [output.bnf]", args[0]);
        eprintln!();
        eprintln!("Arguments:");
        eprintln!("  source       : Source file path or inline code (required)");
        eprintln!("  input.bnf    : Input grammar file (default: Grammar/input.bnf)");
        eprintln!("  output.bnf   : Output grammar file (default: Grammar/output.bnf)");
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  # Inline source code");
        eprintln!("  {} 'int my_func(int a, float b);'", args[0]);
        eprintln!();
        eprintln!("  # From file");
        eprintln!("  {} source.c", args[0]);
        eprintln!();
        eprintln!("  # With custom grammar files");
        eprintln!("  {} source.c Grammar/custom_in.bnf Grammar/custom_out.bnf", args[0]);
        process::exit(1);
    }

    // Grammarディレクトリとファイルの確認・作成
    ensure_grammar_files();

    let source_arg = &args[1];

    // ソースコードの取得（ファイルパスならファイルを読み込む）
    let (source, source_name) = if Path::new(source_arg).exists() {
        let content = fs::read_to_string(source_arg).unwrap_or_else(|e| {
            eprintln!("Error reading source file {}: {}", source_arg, e);
            process::exit(1);
        });
        (content, source_arg.to_string())
    } else {
        (source_arg.to_string(), "<inline>".to_string())
    };

    // BNFファイルパスの決定
    let input_bnf_path = args.get(2)
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}/{}", GRAMMAR_DIR, DEFAULT_INPUT_BNF));

    let output_bnf_path = args.get(3)
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}/{}", GRAMMAR_DIR, DEFAULT_OUTPUT_BNF));

    // BNFファイルの読み込み
    let input_bnf = fs::read_to_string(&input_bnf_path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", input_bnf_path, e);
        process::exit(1);
    });

    let output_bnf = fs::read_to_string(&output_bnf_path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", output_bnf_path, e);
        process::exit(1);
    });

    // Step 1: 入力BNFをパース
    let mut input_meta_parser = MetaParser::new(&input_bnf);
    let input_grammar = input_meta_parser.parse_input_grammar();

    // Step 2: 出力BNFをパース
    let mut output_meta_parser = MetaParser::new(&output_bnf);
    let output_grammar = output_meta_parser.parse_output_grammar();

    // Step 3: ソースコードをパースしてAST生成
    let mut source_parser = Parser::new(&input_grammar, &source);
    let ast = match source_parser.parse() {
        Ok(ast) => ast,
        Err(err) => {
            eprintln!("Error in {}:", source_name);
            eprintln!("{}", err);
            process::exit(1);
        }
    };

    // Step 4: ASTから出力コード生成
    let gen = Generator::new(&output_grammar);
    let output = gen.generate(&ast);

    println!("{}", output);
}
