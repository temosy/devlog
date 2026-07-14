# devlog 使い方

Claude Code のセッション記録と git ログから日報を自動生成する CLI ツールです。

処理は完全ローカルで行われ、セッション内容やコードが外部に出ることはありません（通信先はローカルの Ollama のみ）。

English version: [USAGE.md](USAGE.md)

## 仕組み

1. `~/.claude/projects/**/*.jsonl`（Claude Code のセッション記録）から、その日のセッションタイトル・ユーザーの依頼文・編集ファイル・実行コマンドを抽出します。tool の実行結果や skill の注入文などのノイズは自動で除外されます
2. セッションの作業ディレクトリと編集ファイルのパスから git リポジトリを自動発見し、その日のコミットを収集します
3. ローカルの Ollama（デフォルト: qwen2.5:14b）でプロジェクト別の箇条書きに要約し、Markdown を出力します

## インストール

```sh
git clone https://github.com/temosy/devlog
cd devlog
cargo install --path .
```

前提:

- `git` が PATH にあること
- [Ollama](https://ollama.com) が起動しており、モデルを pull 済みであること（`ollama pull qwen2.5:14b`）。`--no-llm` で使う場合は不要

## 基本コマンド

```sh
devlog today                        # 今日の日報を標準出力へ
devlog today --date 2026-07-10      # 日付を指定
devlog today --out 日報/2026-07-13.md   # ファイルに書き出し（フォルダは自動作成）
devlog today --dry-run              # 収集される生データの確認だけ（LLM を呼ばない・速い）
devlog today --no-llm               # 要約せず生データをそのまま整形出力
devlog today --lang en              # 英語で出力（デフォルトは日本語）
```

要約はローカル 14B モデルのため、1 日分で数分かかることがあります。内容だけ素早く見たいときは `--dry-run` が便利です。

## テンプレート

```sh
devlog today --template daily      # 日報（デフォルト）: プロジェクト別の箇条書き
devlog today --template standup    # スタンドアップ: やったこと / 進行中 / ブロッカー
devlog today --template weekly     # 週報: 指定日までの 7 日間。ハイライト + プロジェクト別
```

週報の例（今日までの 1 週間）: `devlog today --template weekly --out 週報/2026-W28.md`

## 日々の運用（例）

Obsidian vault などのノートフォルダに日付ファイルで貯める運用がおすすめです:

```sh
devlog today --out ~/vault/日報/$(date +%F).md
```

- 作業を終えたタイミングで 1 日 1 回実行する
- 生成後に一読して、事実と違う行があれば手で直す

## 設定ファイル（任意）

無くても動きます。変えたいときだけ `~/.config/devlog/config.toml` を作成してください:

```toml
# すべて省略可。以下はデフォルト値
claude_projects_dir = "~/.claude/projects"
repos = []                            # 自動発見に加えて常に見るリポジトリ（例: ["~/projects/myrepo"]）
ollama_url = "http://localhost:11434"
model = "qwen2.5:14b"
lang = "ja"
```

リポジトリの自動発見は「その日 Claude Code で触ったリポジトリ」しか見つけられません。手作業だけで進めた日のコミットも拾いたいリポジトリは `repos` に列挙しておいてください。

## トラブルシューティング

- **`devlog: ollama not reachable ...` と出て生データが出力される** → Ollama が起動していません。`ollama serve` を起動するか、そのまま生データ出力として使ってください
- **`no activity found` と出る** → その日のセッション・コミットが本当に無いか、日付指定ミスです。`--dry-run` で収集状況を確認してください
- **生成が遅い** → `config.toml` の `model` を軽いモデル（例: `qwen2.5:7b`、要 `ollama pull`）に変えると速くなりますが、要約品質は落ちます

## 既知の制限

- 見出しの言語（「プロジェクト:」/「Project:」）が揺れることがあります（LLM 出力の揺れ）
- リポジトリ外のディレクトリでセッションを開いて別リポジトリを編集した場合、作業のプロジェクト帰属がセッション側のディレクトリ名に寄ることがあります
- 現時点のデータソースは Claude Code のトランスクリプトのみです（Codex 等は今後対応予定）
