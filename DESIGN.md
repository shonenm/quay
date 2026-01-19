# Quay - Port Manager TUI

## 概要

ローカルプロセス、SSHポートフォワード、Dockerコンテナのポートを統合管理するTUIツール。

## 技術スタック

- **言語**: Rust
- **TUI**: ratatui + crossterm
- **非同期**: tokio
- **CLI**: clap

## 機能

| 機能 | 説明 |
|------|------|
| ポート一覧 | Local / SSH / Docker を統合表示 |
| フィルタ/検索 | ポート番号、プロセス名で絞り込み |
| SSH転送作成 | インタラクティブにポートフォワード作成 |
| プロセス停止 | 選択したポートのプロセスを kill |
| 自動更新 | 定期的にポート情報を再取得 |

## データモデル

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortSource {
    Local,      // ローカルプロセス
    Ssh,        // SSH ポートフォワード
    Docker,     // Docker コンテナ
}

#[derive(Debug, Clone)]
pub struct PortEntry {
    pub source: PortSource,
    pub local_port: u16,
    pub remote_host: Option<String>,    // SSH/Docker の場合
    pub remote_port: Option<u16>,       // SSH/Docker の場合
    pub process_name: String,
    pub pid: Option<u32>,
    pub container_id: Option<String>,   // Docker の場合
    pub container_name: Option<String>, // Docker の場合
}
```

## データ取得方法

### Local Ports (macOS)

```bash
lsof -i -P -n | grep LISTEN
```

出力例:
```
node      1234  user   23u  IPv4 0x...  TCP *:3000 (LISTEN)
python    5678  user   5u   IPv6 0x...  TCP *:8080 (LISTEN)
```

### SSH Port Forwards

```bash
ps aux | grep 'ssh.*-[LR]'
```

または状態ファイルで管理:
```
~/.local/state/quay/ssh_forwards.json
```

### Docker Ports

```bash
docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}'
```

出力例:
```
abc123  postgres  0.0.0.0:5432->5432/tcp
def456  redis     0.0.0.0:6379->6379/tcp
```

## UI レイアウト

```
┌─────────────────────────────────────────────────────────────────┐
│ Quay - Port Manager                              [q]uit [?]help │
├─────────────────────────────────────────────────────────────────┤
│ Filter: _                                           [/] search  │
├─────────────────────────────────────────────────────────────────┤
│ TYPE   │ LOCAL  │ REMOTE          │ PROCESS/CONTAINER           │
├────────┼────────┼─────────────────┼─────────────────────────────┤
│ LOCAL  │ :3000  │                 │ node (pid:1234)             │
│ LOCAL  │ :8080  │                 │ python (pid:5678)           │
│ SSH    │ :9000  │ remote:80       │ ssh (pid:2345)              │
│ DOCKER │ :5432  │ postgres:5432   │ postgres (abc123)           │
│ DOCKER │ :6379  │ redis:6379      │ redis (def456)              │
│        │        │                 │                             │
├─────────────────────────────────────────────────────────────────┤
│ [Enter] Details  [k] Kill  [f] Forward  [r] Refresh             │
└─────────────────────────────────────────────────────────────────┘
```

## キーバインド

| キー | アクション |
|------|-----------|
| `j` / `↓` | 下に移動 |
| `k` / `↑` | 上に移動 |
| `g` / `Home` | 先頭に移動 |
| `G` / `End` | 末尾に移動 |
| `/` | 検索モード |
| `Enter` | 詳細表示 |
| `K` | 選択したポートを kill |
| `f` | SSH転送を作成 |
| `r` | リフレッシュ |
| `1` | Local のみ表示 |
| `2` | SSH のみ表示 |
| `3` | Docker のみ表示 |
| `0` | 全て表示 |
| `q` / `Esc` | 終了 |
| `?` | ヘルプ表示 |

## ファイル構成

```
quay/
├── Cargo.toml
├── DESIGN.md
├── README.md
└── src/
    ├── main.rs           # エントリーポイント、CLI引数
    ├── app.rs            # アプリケーション状態
    ├── ui.rs             # UI描画
    ├── event.rs          # キーボードイベント処理
    ├── port/
    │   ├── mod.rs        # PortEntry, PortSource
    │   ├── local.rs      # lsof パース
    │   ├── ssh.rs        # SSH転送管理
    │   └── docker.rs     # docker ps パース
    └── action.rs         # kill, forward などのアクション
```

## CLI インターフェース

```bash
# TUI 起動（デフォルト）
quay

# 一覧表示（非TUI）
quay list
quay list --json
quay list --local
quay list --ssh
quay list --docker

# SSH転送作成
quay forward 8080:localhost:80 remote-host
quay forward -R 9000:localhost:3000 remote-host  # リモート転送

# プロセス停止
quay kill 3000        # ポート番号で
quay kill --pid 1234  # PIDで
```

## 実装ステップ

### Phase 1: 基盤
1. [x] プロジェクト構造作成
2. [x] CLI引数パース (clap)
3. [x] 基本的なTUIフレームワーク (ratatui)
4. [x] イベントループ

### Phase 2: データ取得
5. [x] Local ports (lsof パース)
6. [x] Docker ports (docker ps パース)
7. [x] SSH forwards (ps パース + 状態管理)

### Phase 3: UI
8. [x] テーブル表示
9. [x] フィルタ/検索
10. [x] 詳細ポップアップ
11. [x] ヘルプ画面

### Phase 4: アクション
12. [x] Kill process
13. [x] Create SSH forward
14. [x] Remove SSH forward

### Phase 5: 仕上げ
15. [x] エラーハンドリング
16. [x] 自動更新
17. [x] README
18. [x] Homebrew formula (cargo-dist)

### Phase 6: 拡張機能
19. [x] 設定ファイル基盤 (config.rs)
    - `~/.config/quay/config.toml`
    - auto_refresh, refresh_interval, default_filter, mouse_enabled
20. [x] SSHフォワードプリセット (preset.rs)
    - `~/.config/quay/presets.toml`
    - `p` キーでプリセット一覧表示
21. [x] マウスサポート
    - クリックで行選択、スクロールでリスト移動
22. [x] ドキュメント更新

## 参考

- [ratatui](https://github.com/ratatui/ratatui) - Rust TUI library
- [crossterm](https://github.com/crossterm-rs/crossterm) - Terminal manipulation
- [clap](https://github.com/clap-rs/clap) - CLI argument parser
