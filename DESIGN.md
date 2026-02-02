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
| リモートモード | `--remote host` でSSH経由のリモートポートスキャン＋フォワード |
| フィルタ/検索 | ポート番号、プロセス名で絞り込み |
| SSH転送作成 | インタラクティブにポートフォワード作成 |
| Quick Forward | `F` キーで選択ポートを同一番号でそのままフォワード（リモート/Dockerターゲットモード） |
| Docker ターゲット | `--docker container` でコンテナ内 LISTEN ポートを発見・フォワード |
| プロセス停止 | 選択したポートのプロセスを kill |
| 自動更新 | 定期的にポート情報を再取得 |
| プリセット | SSH転送テンプレートをワンキーで起動 |
| 接続マネージャ | 複数ホスト (Local/Remote/Docker) を `c` で管理、`h`/`l` で切替 |
| マウスサポート | クリック・スクロール操作（設定で有効化） |
| 設定ファイル | auto_refresh, refresh_interval, default_filter, remote_host, docker_target, mouse_enabled, connections.toml |

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
    pub ssh_host: Option<String>,       // SSH転送のホスト
    pub is_open: bool,                  // TCP probe 結果 (リモートモード時はlsof結果を信頼)
    pub is_loopback: bool,             // 127.0.0.1 バインド (Docker ターゲット時)
}
```

## データ取得方法

### Local Ports (macOS / Linux)

```bash
# ローカル
lsof -i -P -n -sTCP:LISTEN -Fcpn

# リモートモード
ssh host "lsof -i -P -n -sTCP:LISTEN -Fcpn"
```

出力例（フィールドベース形式）:
```
p12345      # PID
cnode       # Command name
n*:3000     # Network address
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
# ローカル
docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}'

# リモートモード
ssh host "docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}'"
```

出力例:
```
abc123  postgres  0.0.0.0:5432->5432/tcp
def456  redis     0.0.0.0:6379->6379/tcp
```

### Docker ターゲット (コンテナ内ポート)

```bash
# リモートモード
ssh host "docker exec CONTAINER ss -tln"

# ローカル (将来対応)
docker exec CONTAINER ss -tln
```

出力例:
```
State  Recv-Q Send-Q  Local Address:Port   Peer Address:Port Process
LISTEN 0      511           *:3000              *:*
LISTEN 0      511     0.0.0.0:5173        0.0.0.0:*
LISTEN 0      128     127.0.0.1:5432      0.0.0.0:*
LISTEN 0      511        [::]:3000           [::]:*
```

コンテナ IP 取得:
```bash
ssh host "docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' CONTAINER"
```

フォワード:
```bash
ssh -f -N -L local_port:container_ip:container_port host
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
│ [j/k] Navigate  [Enter] Details  [K] Kill  [f] Forward  [p] Presets  [?] Help  [q] Quit│
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
| `F` | Quick Forward（リモート/Dockerターゲットモード時、同一ポート番号でフォワード） |
| `r` | リフレッシュ |
| `1` | Local のみ表示 |
| `2` | SSH のみ表示 |
| `3` | Docker のみ表示 |
| `0` | 全て表示 |
| `a` | 自動更新の切り替え |
| `p` | プリセット表示 |
| `c` | 接続マネージャ |
| `h` | 前の接続に切替 |
| `l` | 次の接続に切替 |
| `q` / `Esc` | 終了 |
| `?` | ヘルプ表示 |

## ファイル構成

```
quay/
├── Cargo.toml
├── DESIGN.md
├── README.md
├── docs/
│   ├── ARCHITECTURE.md   # アーキテクチャ詳細
│   └── DEVELOPMENT.md    # 開発ガイド
└── src/
    ├── main.rs           # エントリーポイント、CLI引数
    ├── app.rs            # アプリケーション状態
    ├── config.rs         # 設定ファイル処理
    ├── connection.rs     # 接続マネージャ (load/save/add/remove)
    ├── preset.rs         # SSHフォワードプリセット
    ├── ui.rs             # UI描画
    ├── event.rs          # キーボード・マウスイベント処理
    ├── port/
    │   ├── mod.rs        # PortEntry, PortSource, collect_all(remote_host, docker_target)
    │   ├── local.rs      # lsof パース
    │   ├── ssh.rs        # SSH転送管理
    │   └── docker.rs     # docker ps パース, collect_from_container(), get_container_ip()
    └── dev/
        ├── mod.rs        # DevCommands, Scenario定義, run_scenario()
        ├── listen.rs     # spawn_listeners(), TCPリスナー起動
        ├── check.rs      # ポート開閉チェック
        └── mock.rs       # モックデータでTUI起動
```

## CLI インターフェース

```bash
# TUI 起動（デフォルト）
quay

# リモートモード（SSH経由でポートスキャン）
quay --remote user@server
quay --remote user@server list
quay --remote user@server list --json
quay --remote user@server kill 3000

# Docker ターゲット（コンテナ内ポート発見・フォワード）
quay --remote ailab --docker syntopic-dev
quay --remote ailab --docker syntopic-dev list
quay --remote ailab --docker syntopic-dev list --json
quay -r ailab -d syntopic-dev   # 短縮形

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

# 開発・テストツール
quay dev mock                   # モックデータでTUI起動
quay dev scenario full          # シナリオでTUI起動 (open/closed混在)
quay dev scenario web           # Web + DB + Cache シナリオ
quay dev scenario --list        # シナリオ一覧
quay dev listen 4000 5000       # 指定ポートでTCPリスナー起動
quay dev listen 8080 --http     # HTTP応答付きリスナー
quay dev check 3000 8080        # ポート開閉チェック
```

### dev scenario の動作

`run_scenario()` は以下の順序で動作する:

1. `should_listen: true` のポートに対して `spawn_listeners()` でバックグラウンドリスナーを起動（ベストエフォート）
2. シナリオ全エントリから `PortEntry` を生成（open/closed 両方）
3. `run_tui_with_entries()` で TUI を起動
4. TUI 終了時にリスナーを abort

ポートが既に使用中の場合でも TUI は起動し、シナリオ定義に基づいた全エントリが表示される。

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

### Phase 7: リモートモード
23. [x] リモートホスト設定 (config.rs, CLI `--remote` フラグ)
24. [x] SSH経由のポート収集 (local.rs, docker.rs)
25. [x] リモート対応 kill (port/mod.rs)
26. [x] Quick Forward `F` キー (event.rs, main.rs)
27. [x] リモートモード UI (ui.rs — ヘッダー、フッター、ヘルプ、フォーム)

### Phase 8: Docker ターゲットモード
28. [x] Docker ターゲット CLI フラグ (`--docker` / `-d`)
29. [x] コンテナ内ポート発見 (`docker exec ... ss -tln`, docker.rs)
30. [x] コンテナ IP 取得 (`docker inspect`, docker.rs)
31. [x] collect_all() に docker_target 引数追加 (port/mod.rs)
32. [x] App に docker_target, container_ip 追加 (app.rs)
33. [x] config.toml に docker_target 対応 (config.rs)
34. [x] Quick Forward — container_ip 経由のトンネル (main.rs)
35. [x] Forward Form — Remote Host / SSH Host ロック (event.rs, ui.rs)
36. [x] Kill — docker exec kill (main.rs)
37. [x] Docker ターゲット UI 表示 (ui.rs — ヘッダー、フッター、ヘルプ、フォーム)

### Phase 9: 接続マネージャ
38. [x] Connection 構造体 + Connections ラッパー (connection.rs)
    - `~/.config/quay/connections.toml`
    - load/save/add/remove、Local 自動挿入
39. [x] App に接続管理フィールド追加 (app.rs)
    - Popup::Connections, ConnectionPopupMode, ConnectionField, ConnectionInput
    - next_connection, prev_connection, apply_connection
40. [x] イベント処理 (event.rs)
    - `c` → ShowConnections, `h` → PrevConnection, `l` → NextConnection
    - handle_connection_key, handle_connection_input_key
41. [x] TUI ループ統合 (main.rs)
    - 接続ロード + CLI 引数マッチング
    - Connections ポップアップハンドラ (List + AddNew)
    - h/l 接続切替 (ポート再収集 + container IP 解決)
42. [x] UI (ui.rs)
    - ヘッダーにタブ風表示、draw_connections_popup, draw_connection_add_form
    - Help に Connections セクション、Footer に [h/l] Switch
43. [x] Mock モードにサンプル接続追加 (main.rs)

## 参考

- [ratatui](https://github.com/ratatui/ratatui) - Rust TUI library
- [crossterm](https://github.com/crossterm-rs/crossterm) - Terminal manipulation
- [clap](https://github.com/clap-rs/clap) - CLI argument parser
