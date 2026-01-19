# APT Repository Setup Guide

QuayのAPTリポジトリはGitHub Pagesでホストされています。

## ユーザー向け: インストール方法

```bash
# GPG鍵をダウンロード
curl -fsSL https://shonenm.github.io/quay/quay-archive-keyring.gpg | sudo gpg --dearmor -o /usr/share/keyrings/quay-archive-keyring.gpg

# リポジトリを追加
echo "deb [signed-by=/usr/share/keyrings/quay-archive-keyring.gpg] https://shonenm.github.io/quay/ stable main" | sudo tee /etc/apt/sources.list.d/quay.list

# インストール
sudo apt update
sudo apt install quay
```

## メンテナー向け: GPG鍵のセットアップ

### 1. GPG鍵の生成

```bash
# 鍵を生成（パスフレーズなし）
gpg --batch --gen-key <<EOF
Key-Type: RSA
Key-Length: 4096
Name-Real: Quay APT Repository
Name-Email: apt@quay.dev
Expire-Date: 0
%no-protection
EOF

# 鍵IDを確認
gpg --list-secret-keys --keyid-format LONG
```

### 2. 秘密鍵をエクスポート

```bash
# 秘密鍵をエクスポート（GitHub Secretsに保存）
gpg --armor --export-secret-keys "Quay APT Repository" > private-key.asc

# ファイルの内容をコピー
cat private-key.asc

# 秘密鍵ファイルを削除
rm private-key.asc
```

### 3. GitHub Secretsに登録

1. GitHub リポジトリの Settings → Secrets and variables → Actions
2. New repository secret をクリック
3. Name: `APT_GPG_PRIVATE_KEY`
4. Value: エクスポートした秘密鍵の内容を貼り付け

### 4. GitHub Pagesを有効化

1. Settings → Pages
2. Source: Deploy from a branch
3. Branch: gh-pages / (root)
4. Save

## アーキテクチャ

```
gh-pages branch
├── index.html                    # インストール手順
├── quay-archive-keyring.gpg      # GPG公開鍵
├── pool/
│   └── main/
│       ├── quay_0.1.0_amd64.deb
│       └── quay_0.1.0_arm64.deb
└── dists/
    └── stable/
        ├── Release
        ├── Release.gpg
        ├── InRelease
        └── main/
            ├── binary-amd64/
            │   ├── Packages
            │   └── Packages.gz
            └── binary-arm64/
                ├── Packages
                └── Packages.gz
```

## トラブルシューティング

### GPG署名エラー

`APT_GPG_PRIVATE_KEY` シークレットが設定されていない場合、署名なしでデプロイされます。
署名付きリポジトリにするには、上記のGPG鍵セットアップを完了してください。

### ビルドエラー

`cargo-deb` のビルドエラーが発生した場合:

```bash
# ローカルでテスト
cargo install cargo-deb
cargo deb --target x86_64-unknown-linux-gnu
```
