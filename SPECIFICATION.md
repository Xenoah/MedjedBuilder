# Html2Apk v0.1 仕様

## 対象

- 生成ツール: Windows 10/11 x64
- 生成アプリ: Android 8.0（API 26）以降
- 出力: インストール可能な署名済みAPK
- 利用時依存: なし（Java、SDK、Gradle、Python、ネット接続不要）

## 変換方式

1. CIでAndroid WebViewテンプレートAPKを作成
2. Rustの`include_bytes!`でテンプレートをEXEへ埋め込み
3. HTMLフォルダと`app.json`をAPKへ注入
4. バイナリAndroidManifest（AXML）の文字列プールと型付き属性を更新
5. ZIPを再構築し、無圧縮エントリを4バイト境界へ整列
6. RSA-3072鍵と自己署名X.509証明書でAPK Signature Scheme v2署名
7. 出力したAPKの署名を再検証

## GUI設定

- HTMLフォルダ
- 出力APK
- アプリ名
- パッケージID
- versionName / versionCode
- 開始ページ
- アイコン
- 画面向き（自動・縦・横）
- 全画面
- スリープ防止
- 外部URLを既定ブラウザで開く
- HTTP通信許可
- ステータスバー色 / ナビゲーションバー色
- ストレージモード
- ファイルAPI
- インターネット、カメラ、マイク、位置情報、通知
- 既存署名鍵の指定

## ストレージモード

| モード | 用途 | Android API |
|---|---|---|
| アプリ専用 | 設定・セーブデータ | `filesDir/h2a` |
| メディア | 音楽・画像・動画 | MediaStore |
| 選択フォルダ | 一般ファイル操作 | Storage Access Framework |
| 全ファイル | ファイルマネージャー | `MANAGE_EXTERNAL_STORAGE`または旧ストレージ権限 |

## JavaScriptブリッジ

- APIはローカルHTMLからのみ使用可能
- 全メソッドはPromiseを返す
- `../`、絶対パス、シンボリックリンクによる範囲外アクセスを拒否
- SAFパスは`saf:`、全ファイルパスは`ext:`を付与
- cordova-safe形式の`encrypt(file,key,success,error)`と`decrypt(...)`を提供
- 暗号形式はAES-256-GCM、PBKDF2-HMAC-SHA256（150,000回）、ランダムsalt/nonce

## 署名鍵

- 初回にパッケージIDごとの鍵を自動生成
- 通常は「ドキュメント/Html2Apk/keys」に保存
- 同じ鍵を再利用して既存アプリを更新
- `.h2akey`の指定による移行・復元に対応

## セキュリティ

- 外部URLは原則として端末ブラウザで開く
- 外部ページではネイティブブリッジを拒否
- WebViewデバッグを無効化
- HTTPは初期状態で禁止
- 不要なAndroid権限を無効化
- アプリバックアップを無効化
- HTML内へ秘密鍵や認証情報を格納しないことを前提とする

## 対象外

- AAB / Google Play向け公開フロー
- iOS
- Android 7以前
- DRM
- HTMLソースの難読化・秘匿
- Android WebView本体の同梱

