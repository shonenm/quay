# 持続可能なOSSアーキテクチャ：Quayブループリント

## 1. 概要

Quayを個人のユーティリティから、コミュニティ主導の持続可能なOSSへと昇華させるための技術的・組織的基盤の設計書。

### 目標
- **配布の簡素化**: Time-to-Hello-Worldの最小化
- **メンテナンスの自動化**: リリース作業の人的コスト削減
- **セキュリティの担保**: サプライチェーン攻撃への対策

---

## 2. 配布・パッケージ管理戦略

### 2.1 Crates.io

#### 名前空間とメタデータ

```toml
[package]
name = "quay-tui"  # クレート名（競合回避）
version = "0.1.0"
default-run = "quay"  # デフォルトのバイナリ名

[[bin]]
name = "quay"
path = "src/main.rs"

# 検索可能性向上
categories = ["command-line-utilities", "network-programming"]
keywords = ["tui", "port-manager", "docker", "ssh", "ratatui"]
```

### 2.2 cargo-dist によるバイナリ配布

#### ビルドターゲット

| ターゲット | 用途 |
|-----------|------|
| `x86_64-apple-darwin` | macOS Intel |
| `aarch64-apple-darwin` | macOS Apple Silicon |
| `x86_64-unknown-linux-musl` | Linux (静的リンク) |
| `aarch64-unknown-linux-musl` | Linux ARM |

**推奨**: Linux向けには **musl** ターゲットを使用（静的リンクで移植性向上）

#### シェルインストーラー

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/username/quay/releases/download/v0.1.0/quay-installer.sh | sh
```

### 2.3 Homebrew (macOS)

#### Custom Tap アーキテクチャ

```bash
brew tap username/quay
brew install quay
```

cargo-dist が Formula を自動生成・更新:
1. GitHub Releases へアーカイブアップロード
2. SHA256 ハッシュ取得
3. `quay.rb` Formula 生成
4. Tap リポジトリへ自動コミット

### 2.4 APT リポジトリ (Linux)

#### GitHub Pages によるサーバーレス運用

```bash
# ユーザー側の設定
echo "deb [signed-by=/usr/share/keyrings/quay-archive-keyring.gpg] https://username.github.io/quay/ stable main" | sudo tee /etc/apt/sources.list.d/quay.list
```

#### 実装手順
1. GPG鍵の生成（秘密鍵をGitHub Secrets、公開鍵をリポジトリへ）
2. `cargo-deb` で `.deb` パッケージ生成
3. `dpkg-scanpackages` でインデックス生成
4. GPG署名（Release.gpg, InRelease）
5. gh-pages ブランチにデプロイ

---

## 3. リリースフロー自動化

### 3.1 Release PR パターン (release-plz)

```
開発 → feat: add feature
        ↓
release-plz検知 → PR作成 (chore: release v0.2.0)
        ↓
メンテナがマージ → タグ作成 → Crates.io公開
        ↓
cargo-dist起動 → バイナリビルド → GitHub Release作成
```

### 3.2 役割分担

| ツール | 担当 |
|--------|------|
| release-plz | タグ作成、Changelog生成、Crates.io公開 |
| cargo-dist | バイナリビルド、GitHub Release作成、Homebrew更新 |

### 3.3 Conventional Commits

```
feat: 新機能 → マイナーバージョンアップ
fix: バグ修正 → パッチバージョンアップ
docs: ドキュメント
chore: メンテナンス
```

---

## 4. 依存関係管理

### 4.1 Renovate vs Dependabot

**Renovate を推奨** する理由:
- 関連パッケージのグルーピング（ratatui + crossterm 等）
- DevDependencies の自動マージ
- Dependency Dashboard による一元管理

### 4.2 グルーピング例

```json
{
  "packageRules": [
    {
      "matchPackagePatterns": ["^ratatui", "^crossterm", "^tui-"],
      "groupName": "tui-stack"
    },
    {
      "matchPackagePatterns": ["^tokio"],
      "groupName": "async-runtime"
    }
  ]
}
```

### 4.3 セキュリティ監査

| ツール | 用途 |
|--------|------|
| cargo-audit | RustSec Advisory Database との照合 |
| cargo-deny | ライセンスチェック、重複依存の検出 |

---

## 5. 設定ファイル例

### 5.1 release-plz.toml

```toml
[workspace]
dependencies_update = true
allow_dirty = false

[workspace.changelog]
config = "cliff.toml"

[[package]]
name = "quay"
git_tag_enable = true
git_release_enable = false  # cargo-distに任せる
publish = true
```

### 5.2 Cargo.toml (cargo-dist)

```toml
[workspace.metadata.dist]
cargo-dist-version = "0.10.0"
ci = "github"
targets = [
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin"
]
installers = ["shell", "homebrew"]
tap = "username/homebrew-quay"
publish-jobs = ["homebrew"]
checksum = "sha256"
```

### 5.3 renovate.json

```json
{
  "$schema": "https://docs.renovatebot.com/renovate-schema.json",
  "extends": ["config:base"],
  "postUpdateOptions": ["gomodTidy"],
  "rangeStrategy": "bump",
  "packageRules": [
    {
      "matchPackagePatterns": ["^ratatui", "^crossterm"],
      "groupName": "tui-stack"
    },
    {
      "matchDepTypes": ["devDependencies"],
      "matchUpdateTypes": ["minor", "patch"],
      "automerge": true
    },
    {
      "matchUpdateTypes": ["minor", "patch"],
      "groupName": "all non-major dependencies",
      "schedule": ["before 4am on monday"]
    }
  ],
  "lockFileMaintenance": {
    "enabled": true,
    "schedule": ["before 4am on monday"]
  }
}
```

---

## 6. コミュニティ運営

### 6.1 必要なドキュメント

| ファイル | 内容 |
|----------|------|
| CONTRIBUTING.md | 開発環境セットアップ、PR プロセス |
| CODE_OF_CONDUCT.md | Contributor Covenant v2.1 |
| .github/ISSUE_TEMPLATE/ | Issue Forms (YAML) |

### 6.2 Issue Forms 例

```yaml
# .github/ISSUE_TEMPLATE/bug_report.yml
name: Bug Report
body:
  - type: input
    id: version
    attributes:
      label: Quay Version
      placeholder: "0.1.0"
    validations:
      required: true
  - type: dropdown
    id: os
    attributes:
      label: Operating System
      options:
        - macOS (Apple Silicon)
        - macOS (Intel)
        - Linux (Debian/Ubuntu)
  - type: textarea
    id: reproduction
    attributes:
      label: Steps to Reproduce
    validations:
      required: true
```

---

## 7. 実装フェーズ

### Phase 1: 基盤整備
- [x] cargo-dist 設定
- [x] release-plz 設定
- [x] GitHub Actions ワークフロー

### Phase 2: 配布チャネル
- [x] Homebrew Tap リポジトリ作成
- [x] APT リポジトリ (GitHub Pages)
- [x] シェルインストーラー

### Phase 3: 自動化
- [x] Renovate 導入
- [x] cargo-audit CI
- [x] 日次セキュリティスキャン

### Phase 4: コミュニティ
- [x] CONTRIBUTING.md
- [x] CODE_OF_CONDUCT.md
- [x] Issue Forms

---

## 8. 参考ツール

| ツール | 用途 | URL |
|--------|------|-----|
| cargo-dist | バイナリ配布 | https://opensource.axo.dev/cargo-dist/ |
| release-plz | リリース自動化 | https://release-plz.ieni.dev/ |
| cargo-deb | Debian パッケージ | https://github.com/kornelski/cargo-deb |
| git-cliff | Changelog 生成 | https://git-cliff.org/ |
| Renovate | 依存関係管理 | https://docs.renovatebot.com/ |
| cargo-audit | セキュリティ監査 | https://github.com/RustSec/rustsec |
