// ─── User configuration (config.toml) ─────────────────────
//
// 起動時に 1 回だけ読み込むユーザー設定。現状の項目は拡張子別の
// 外部オープナー([[openers]])のみ。設定ファイルが無い・壊れている
// 場合は空設定で継続し(fail-open)、従来挙動を完全に維持する。
//
// 置き場所: <dirs::config_dir()>/ccmux/config.toml
//   Windows: %APPDATA%\ccmux\config.toml
//   Linux:   ~/.config/ccmux/config.toml
//   macOS:   ~/Library/Application Support/ccmux/config.toml
//
// 例:
//   [[openers]]
//   extensions = ["md", "markdown"]
//   command = "typora"            # Windows の .cmd シムは "code.cmd" のように拡張子まで書く
//   args = ["{file}"]             # {file} = 絶対パス。省略時は末尾に自動付加

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// オープナーの起動方式。
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OpenerMode {
    /// `command` を直接 spawn する(既定。従来どおりの挙動)。
    #[default]
    Command,
    /// OS の既定アプリに委ねる。`command` / `args` は使わない。
    /// ストアアプリ(MSIX)のようにインストールパスへバージョン番号が
    /// 入るアプリを、パスを書かずに起動するための方式。
    Default,
}

/// 拡張子と外部コマンドの対応 1 件(config.toml の [[openers]])。
#[derive(Debug, Clone, Deserialize)]
pub struct Opener {
    /// 対象拡張子(ドットなし)。比較は小文字化して行う。
    /// 判定はファイル名の末尾一致(ドット境界)なので "tar.gz" のような
    /// 複合拡張子も書ける。より長い一致が優先される。
    #[serde(default)]
    pub extensions: Vec<String>,
    /// 拡張子を持たないファイル(Makefile 等)向けの完全一致名。
    /// 小文字化して比較し、extensions のどのマッチよりも優先される。
    #[serde(default)]
    pub filenames: Vec<String>,
    /// 起動コマンド。PATH 解決は OS に任せる(シェルは経由しない)。
    /// mode = "default" では不要。
    #[serde(default)]
    pub command: Option<String>,
    /// 起動方式。省略時は command 方式(従来の挙動)。
    #[serde(default)]
    pub mode: OpenerMode,
    /// 引数。"{file}" は対象ファイルの絶対パスに置換され、
    /// どの引数にも現れない場合は末尾に自動付加される。
    /// mode = "default" では無視される。
    #[serde(default)]
    pub args: Vec<String>,
}

impl Opener {
    /// 起動に使えるオープナーか。mode = "command" なのに command が
    /// 書かれていない設定は spawn しようがないためマッチ対象から外す。
    pub fn is_usable(&self) -> bool {
        self.mode == OpenerMode::Default || self.command.is_some()
    }

    /// 小文字化済みファイル名に対するマッチ強度(大きいほど優先)。
    /// None は非マッチ。filenames の完全一致を最優先とし、extensions は
    /// ".<ext>" での末尾一致(ドット境界)にマッチした最長の拡張子長を返す。
    /// ドット境界を要求するので "atar.gz" は "tar.gz" にマッチしない。
    fn match_score(&self, lower_name: &str) -> Option<usize> {
        if self.filenames.iter().any(|f| f.to_lowercase() == lower_name) {
            return Some(usize::MAX);
        }
        // 判定とスコアを同じ小文字化結果から取る(大文字小文字で
        // 長さが変わる文字を書かれてもスコアが実際の一致長とずれない)
        self.extensions
            .iter()
            .filter_map(|e| {
                let e = e.to_lowercase();
                lower_name.ends_with(&format!(".{}", e)).then(|| e.chars().count())
            })
            .max()
    }
}

/// scrollback_lines の既定値(ペインごとの vt100 スクロールバック行数)。
/// 旧来の 10,000 行は満杯時 ≈38MB/ペイン(120 桁換算)とメモリ支配項
/// だったため 2,000 行(≈7.7MB)に変更。長い履歴が必要なら config.toml で戻せる。
const DEFAULT_SCROLLBACK_LINES: usize = 2_000;
/// scrollback_lines の上限。120 桁 × 32byte/セル換算で約 384MB/ペインに
/// 相当するため、これを超える指定はサイレントにクランプする。
const MAX_SCROLLBACK_LINES: usize = 100_000;

fn default_scrollback_lines() -> usize {
    DEFAULT_SCROLLBACK_LINES
}

/// ccmux のユーザー設定。
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub openers: Vec<Opener>,
    /// ペインごとのスクロールバック行数。0 は履歴なし。
    /// vt100 Parser 生成時に固定されるため反映は起動時のみ。
    #[serde(default = "default_scrollback_lines")]
    pub scrollback_lines: usize,
}

// derive(Default) だと scrollback_lines が 0 になり serde の
// default(= DEFAULT_SCROLLBACK_LINES)と食い違うため手書きで一致させる。
impl Default for Config {
    fn default() -> Self {
        Self {
            openers: Vec::new(),
            scrollback_lines: default_scrollback_lines(),
        }
    }
}

impl Config {
    /// 設定ファイルを読み込む。戻り値の .1 は起動を止めない一時
    /// メッセージ(パース失敗の警告 / 初回の雛形生成通知 /
    /// command 未指定オープナーを無視した旨の警告)。
    /// ファイル不在時は全コメントの雛形を生成して通知を返し(失敗時は
    /// 無通知)、NotFound 以外の読み取りエラーは空設定・無通知で継続。
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = config_path() else {
            return (Self::default(), None);
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match Self::parse(&text) {
                Ok(cfg) => {
                    // 設定は読めたが起動不能なオープナーがあれば併せて通知する
                    let warning = cfg.unusable_opener_warning();
                    (cfg, warning)
                }
                Err(e) => (
                    Self::default(),
                    Some(format!("config.toml 読み込み失敗(設定を無視して継続): {}", e)),
                ),
            },
            // 不在時のみ雛形を生成(生成通知が .1 に乗る)。NotFound 以外の
            // エラー(権限拒否等)では書き込みに行かず従来どおり空設定で継続。
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (Self::default(), ensure_skel_at(&path))
            }
            Err(_) => (Self::default(), None),
        }
    }

    /// TOML テキストをパースする(load から分離してテスト可能に)。
    /// Windows のエディタが付ける UTF-8 BOM は除去してから渡す。
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        toml::from_str::<Self>(text)
            .map(|mut cfg: Self| {
                // 極端な値はサイレントにクランプ(負値・型不一致は toml 側で
                // Err になり、load の fail-open 経路で既定値+警告に落ちる)。
                cfg.scrollback_lines = cfg.scrollback_lines.min(MAX_SCROLLBACK_LINES);
                cfg
            })
            .map_err(|e| {
                // toml のエラーは複数行になるためステータスバー向けに先頭行だけ残す
                e.to_string().lines().next().unwrap_or("parse error").to_string()
            })
    }

    /// path に対応するオープナーを返す。マッチ強度が最大のものを選び、
    /// 同点なら config.toml の記述順で先に書かれた方を採る。
    /// (filenames 完全一致 > より長い extensions 一致 > 記述順)
    pub fn find_opener(&self, path: &Path) -> Option<&Opener> {
        let name = path.file_name()?.to_str()?.to_lowercase();
        let mut best: Option<(usize, &Opener)> = None;
        for o in self.openers.iter().filter(|o| o.is_usable()) {
            let Some(score) = o.match_score(&name) else {
                continue;
            };
            if best.is_none_or(|(prev, _)| score > prev) {
                best = Some((score, o));
            }
        }
        best.map(|(_, o)| o)
    }

    /// 実行中の再読み込み(`r` キー)で差し替えてよいフィールドだけを適用する。
    ///
    /// `scrollback_lines` を差し替えてはならない。この値は vt100 Parser の
    /// 生成時に固定されるが、Parser を作るのは起動時だけでなく新タブ・
    /// ペイン分割のたびであり、そこで `self.config` から読まれる。構造体ごと
    /// 差し替えると「反映は再起動時」と案内しているのに新規ペインだけ新しい
    /// 値になり、同一セッション内でペインごとに履歴長が食い違う。
    /// リロード対象を増やすときは必ずこのメソッドに追加すること。
    pub fn apply_reload(&mut self, new: Self) {
        self.openers = new.openers;
    }

    /// command 未指定で無視したオープナーがあれば警告文を返す。
    /// 設定ミスを黙って無視しないための通知で、起動は止めない。
    pub fn unusable_opener_warning(&self) -> Option<String> {
        let n = self.openers.iter().filter(|o| !o.is_usable()).count();
        // 呼び出し側("config.toml 再読み込み: …" 等)と語を重ねないよう
        // ファイル名は含めない
        (n > 0).then(|| format!("command 未指定のオープナー {} 件を無視しました", n))
    }
}

/// オープナーの args を実引数へ展開する(mode = "command" 専用)。
/// "{file}" を abs_path に置換し、どの引数にも現れない場合は末尾に付加する。
/// mode = "default" では OS ランチャに追加引数を渡す意味がないため
/// 呼び出し側で分岐し、この関数は通らない。
pub fn build_args(opener: &Opener, abs_path: &str) -> Vec<String> {
    let mut replaced = false;
    let mut out: Vec<String> = opener
        .args
        .iter()
        .map(|a| {
            if a.contains("{file}") {
                replaced = true;
                a.replace("{file}", abs_path)
            } else {
                a.clone()
            }
        })
        .collect();
    if !replaced {
        out.push(abs_path.to_string());
    }
    out
}

pub(crate) fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ccmux").join("config.toml"))
}

/// 初回起動時に生成する設定ファイルの雛形。全コメントのため
/// 生成しても挙動は変わらない(空設定と等価)。
pub(crate) const CONFIG_SKEL: &str = r##"# ccmux 設定ファイル
# (このファイルは初回起動時に自動生成された雛形です。すべてコメントのため
#  そのままでは何も変わりません。必要な設定のコメントを外して使ってください)
#
# [[openers]] = ファイル別・外部オープナー設定(複数書けます)。
# ファイルツリーで選択(Enter / クリック)したファイルがマッチすると外部アプリで
# 開きます。未登録・起動失敗時は従来どおり TUI プレビュー。
#
# ファイルツリーのキー:
#   o = この設定に関係なく OS の既定アプリで開く(ディレクトリも可)
#   r = このファイルを再読み込み([[openers]] のみ反映)
#
# マッチの書き方:
# - extensions はドットなし。ファイル名の末尾一致(ドット境界)なので
#   "tar.gz" のような複合拡張子も書けます。より長い一致が優先されます
# - filenames は拡張子を持たないファイル用の完全一致("Makefile" 等)。
#   extensions のどのマッチよりも優先されます
# - 大文字小文字は区別しません
#
# 起動方式(mode):
# - mode 省略 = command を直接起動。{file} は絶対パスに置換(省略時は末尾に付加)
# - mode = "default" = OS の既定アプリに委ねる(command / args は不要・無視)。
#   ストアアプリ(MSIX)はインストールパスにバージョン番号が入り、アプリ更新の
#   たびに直接パス指定が壊れるため、パスを書かずにこちらを使ってください

# 例1: Markdown を OS の既定アプリで開く(ストアアプリはこの方式が安全)
#[[openers]]
#extensions = ["md", "markdown"]
#mode = "default"

# 例2: Markdown を VS Code で開く
# (Windows の .cmd シム(VS Code 等)は "code.cmd" のように拡張子まで書く)
#[[openers]]
#extensions = ["md", "markdown"]
#command = "code.cmd"
#args = ["{file}"]

# 例3: 拡張子のないファイルを既定アプリで開く
#[[openers]]
#filenames = ["Makefile", "Dockerfile"]
#mode = "default"

# scrollback_lines = ペインごとのスクロールバック行数。既定 2000。
# 0 で履歴なし。上限 100000(超過分は切り詰め)。r キーでは反映されず再起動が必要。
# メモリ目安: 行数 × 画面桁数 × 32byte がペインごとの上限
#scrollback_lines = 2000
"##;

/// path に設定ファイルの雛形を生成する(初回起動時)。
/// 不在判定は呼び出し側(read の NotFound)、作成は create_new で
/// 原子的に行い、競合時も既存ファイルを上書きしない。
/// あらゆる失敗は None(fail-open: 雛形は利便機能であり起動を止めない)。
fn ensure_skel_at(path: &Path) -> Option<String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok()?;
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .ok()?;
    use std::io::Write as _;
    f.write_all(CONFIG_SKEL.as_bytes()).ok()?;
    Some(format!("設定ファイルの雛形を作成しました: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opener(exts: &[&str], args: &[&str]) -> Opener {
        Opener {
            extensions: exts.iter().map(|s| s.to_string()).collect(),
            filenames: Vec::new(),
            command: Some("dummy".to_string()),
            mode: OpenerMode::Command,
            args: args.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// 名前付きコマンドの opener(どれが選ばれたかをテストで見分けるため)。
    fn named(cmd: &str, exts: &[&str], names: &[&str]) -> Opener {
        Opener {
            extensions: exts.iter().map(|s| s.to_string()).collect(),
            filenames: names.iter().map(|s| s.to_string()).collect(),
            command: Some(cmd.to_string()),
            mode: OpenerMode::Command,
            args: Vec::new(),
        }
    }

    fn cmd_of(cfg: &Config, path: &str) -> Option<String> {
        cfg.find_opener(Path::new(path)).map(|o| o.command.clone().unwrap_or_default())
    }

    #[test]
    fn test_parse_valid_toml() {
        let cfg = Config::parse(
            r#"
            [[openers]]
            extensions = ["md", "markdown"]
            command = "typora"
            args = ["{file}"]
            "#,
        )
        .unwrap();
        assert_eq!(cfg.openers.len(), 1);
        assert_eq!(cfg.openers[0].command.as_deref(), Some("typora"));
        assert_eq!(cfg.openers[0].extensions, vec!["md", "markdown"]);
    }

    #[test]
    fn test_parse_invalid_toml_returns_err() {
        // 型不一致(extensions が文字列)はエラーになる
        let err = Config::parse("[[openers]]\nextensions = \"md\"\ncommand = \"x\"").unwrap_err();
        assert!(!err.is_empty());
        // エラーは 1 行に収まっている(ステータスバー表示用)
        assert!(!err.contains('\n'));
    }

    #[test]
    fn test_parse_empty_is_default() {
        let cfg = Config::parse("").unwrap();
        assert!(cfg.openers.is_empty());
    }

    #[test]
    fn test_parse_strips_utf8_bom() {
        let cfg = Config::parse("\u{feff}[[openers]]\nextensions = [\"md\"]\ncommand = \"x\"")
            .unwrap();
        assert_eq!(cfg.openers.len(), 1);
    }

    #[test]
    fn test_find_opener_case_insensitive() {
        let cfg = Config { openers: vec![opener(&["md"], &[])], ..Default::default() };
        assert!(cfg.find_opener(Path::new("README.MD")).is_some());
        assert!(cfg.find_opener(Path::new("note.Md")).is_some());
    }

    #[test]
    fn test_find_opener_unregistered_or_no_ext() {
        let cfg = Config { openers: vec![opener(&["md"], &[])], ..Default::default() };
        assert!(cfg.find_opener(Path::new("main.rs")).is_none());
        assert!(cfg.find_opener(Path::new("Makefile")).is_none());
    }

    #[test]
    fn test_build_args_replaces_placeholder() {
        let o = opener(&["md"], &["--open", "{file}"]);
        assert_eq!(
            build_args(&o, "C:/work/a.md"),
            vec!["--open".to_string(), "C:/work/a.md".to_string()]
        );
    }

    /// テスト用の一意な一時ディレクトリ(tempdir crate 不使用)。
    fn temp_dir_unique(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ccmux-skel-test-{}-{}", tag, std::process::id()))
    }

    #[test]
    fn test_config_skel_parses_to_empty() {
        let cfg = Config::parse(CONFIG_SKEL).unwrap();
        assert!(cfg.openers.is_empty());
        // 全コメントの雛形は既定値(=Default)と等価であること
        assert_eq!(cfg.scrollback_lines, Config::default().scrollback_lines);
    }

    #[test]
    fn test_scrollback_lines_default_when_missing() {
        let cfg = Config::parse("").unwrap();
        assert_eq!(cfg.scrollback_lines, DEFAULT_SCROLLBACK_LINES);
    }

    #[test]
    fn test_scrollback_lines_explicit_value() {
        let cfg = Config::parse("scrollback_lines = 5000").unwrap();
        assert_eq!(cfg.scrollback_lines, 5000);
    }

    #[test]
    fn test_scrollback_lines_zero_allowed() {
        let cfg = Config::parse("scrollback_lines = 0").unwrap();
        assert_eq!(cfg.scrollback_lines, 0);
    }

    #[test]
    fn test_scrollback_lines_clamped_to_max() {
        let cfg = Config::parse("scrollback_lines = 100001").unwrap();
        assert_eq!(cfg.scrollback_lines, MAX_SCROLLBACK_LINES);
    }

    #[test]
    fn test_scrollback_lines_negative_is_parse_err() {
        // 負値は usize へのデシリアライズエラー → load 側の fail-open 経路
        // (既定値+ステータス警告)に乗る。ここでは Err になることを確認。
        let err = Config::parse("scrollback_lines = -1").unwrap_err();
        assert!(!err.is_empty());
        assert!(!err.contains('\n'));
    }

    #[test]
    fn test_ensure_skel_at_creates_when_missing() {
        let dir = temp_dir_unique("create");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.toml");
        assert!(ensure_skel_at(&path).is_some());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), CONFIG_SKEL);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ensure_skel_at_never_overwrites() {
        let dir = temp_dir_unique("keep");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "user content").unwrap();
        assert!(ensure_skel_at(&path).is_none());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "user content");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ensure_skel_at_creates_nested_dirs() {
        let dir = temp_dir_unique("nested");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("a").join("b").join("config.toml");
        assert!(ensure_skel_at(&path).is_some());
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── v2: 末尾一致・filenames・mode ───────────────────

    #[test]
    fn test_find_opener_longest_extension_wins() {
        // 記述順は gz が先。それでも長い一致(tar.gz)が勝つ
        let cfg = Config {
            openers: vec![named("gzip-app", &["gz"], &[]), named("tar-app", &["tar.gz"], &[])],
            ..Default::default()
        };
        assert_eq!(cmd_of(&cfg, "backup.tar.gz").as_deref(), Some("tar-app"));
        // 単独の .gz は短い方にしかマッチしない
        assert_eq!(cmd_of(&cfg, "note.gz").as_deref(), Some("gzip-app"));
    }

    #[test]
    fn test_find_opener_requires_dot_boundary() {
        // "atar.gz" は ".tar.gz" で終わらないので tar.gz にはマッチしない
        let cfg = Config { openers: vec![named("tar-app", &["tar.gz"], &[])], ..Default::default() };
        assert!(cfg.find_opener(Path::new("atar.gz")).is_none());
        assert!(cfg.find_opener(Path::new("a.tar.gz")).is_some());
    }

    #[test]
    fn test_find_opener_filenames_beats_extensions() {
        // Makefile.md のような名前でも、完全一致する filenames が無ければ拡張子側
        let cfg = Config {
            openers: vec![named("ext-app", &["mk"], &[]), named("name-app", &[], &["Makefile.mk"])],
            ..Default::default()
        };
        assert_eq!(cmd_of(&cfg, "Makefile.mk").as_deref(), Some("name-app"));
        assert_eq!(cmd_of(&cfg, "other.mk").as_deref(), Some("ext-app"));
    }

    #[test]
    fn test_find_opener_matches_extensionless_filenames() {
        let cfg = Config {
            openers: vec![named("build-app", &[], &["Makefile", "Dockerfile"])],
            ..Default::default()
        };
        assert!(cfg.find_opener(Path::new("Makefile")).is_some());
        assert!(cfg.find_opener(Path::new("Dockerfile")).is_some());
        // 大文字小文字は区別しない
        assert!(cfg.find_opener(Path::new("MAKEFILE")).is_some());
        assert!(cfg.find_opener(Path::new("Makefile.old")).is_none());
    }

    /// v1 からの意図的な挙動変更: ドットファイルが extensions にマッチする。
    /// v1 は Path::extension() が None を返すため非マッチだった。
    #[test]
    fn test_find_opener_matches_dotfile_by_extension() {
        let cfg = Config { openers: vec![opener(&["gitignore"], &[])], ..Default::default() };
        assert!(cfg.find_opener(Path::new(".gitignore")).is_some());
    }

    #[test]
    fn test_parse_mode_default_and_omitted() {
        let cfg = Config::parse(
            r#"
            [[openers]]
            extensions = ["md"]
            mode = "default"

            [[openers]]
            extensions = ["rs"]
            command = "editor"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.openers[0].mode, OpenerMode::Default);
        assert!(cfg.openers[0].command.is_none());
        // mode 省略時は従来どおり command 方式
        assert_eq!(cfg.openers[1].mode, OpenerMode::Command);
    }

    #[test]
    fn test_find_opener_skips_command_less_command_mode() {
        // mode 省略 + command なしは起動しようがないので無視し、
        // 後続の正しいオープナーが選ばれる
        let cfg = Config::parse(
            r#"
            [[openers]]
            extensions = ["md"]

            [[openers]]
            extensions = ["md"]
            command = "fallback"
            "#,
        )
        .unwrap();
        assert_eq!(cmd_of(&cfg, "a.md").as_deref(), Some("fallback"));
        assert!(cfg.unusable_opener_warning().is_some());
    }

    /// リロードは openers だけを差し替える。scrollback_lines まで
    /// 差し替えると、以降に作られたペインだけ履歴長が変わってしまい
    /// 「反映は再起動時」という案内と食い違う。
    #[test]
    fn test_apply_reload_replaces_openers_but_keeps_scrollback() {
        // 元の openers を 1 件持たせることで「置換」と「追記」を区別する
        let mut cfg = Config {
            openers: vec![named("old-app", &["md"], &[])],
            scrollback_lines: 5000,
        };
        let new = Config::parse(
            r#"
            scrollback_lines = 50000
            [[openers]]
            extensions = ["md"]
            command = "new-app"
            "#,
        )
        .unwrap();
        cfg.apply_reload(new);
        assert_eq!(cfg.openers.len(), 1);
        assert_eq!(cmd_of(&cfg, "a.md").as_deref(), Some("new-app"));
        assert_eq!(cfg.scrollback_lines, 5000);
    }

    #[test]
    fn test_unusable_opener_warning_none_when_all_valid() {
        let cfg = Config::parse("[[openers]]\nextensions = [\"md\"]\nmode = \"default\"").unwrap();
        assert!(cfg.unusable_opener_warning().is_none());
        assert!(cfg.find_opener(Path::new("a.md")).is_some());
    }

    #[test]
    fn test_build_args_appends_when_missing() {
        let o = opener(&["md"], &["--new-window"]);
        assert_eq!(
            build_args(&o, "/tmp/a.md"),
            vec!["--new-window".to_string(), "/tmp/a.md".to_string()]
        );
        let empty = opener(&["md"], &[]);
        assert_eq!(build_args(&empty, "/tmp/a.md"), vec!["/tmp/a.md".to_string()]);
    }
}
