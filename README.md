# XenoBuilder
ーHTML,JS,CSSでAndroidアプリをビルドー

HTMLフォルダを、Windows上で署名済みAndroid APKへ変換するオフラインツールです。利用者のPCにJava、Android SDK、Gradle、Pythonは不要です。

## 主な機能

- HTML/CSS/JavaScriptと素材ファイルをAPKへ内蔵
- 出力形式はAPKとAAB（Google Play提出用）を選択可能
- アプリ名、パッケージID、バージョン、開始ページを設定
- 縦・横・自動回転、全画面、スリープ防止、バー色を設定
- PNG/JPEG/WebPアイコンを各Android密度へ変換
- 必要な権限だけを有効化
- アプリ専用、MediaStore、SAF選択フォルダ、全ファイルの4モード
- AES-256-GCMによるアプリ内ファイル暗号化
- パッケージIDごとのRSA-3072署名鍵を自動生成・再利用
- APKはAPK Signature Scheme v2で署名し、出力後に自己検証
- AABはJAR署名（SHA-256 / RSA）を付与

生成アプリはAndroid 8.0（API 26）以降に対応します。

## 利用者向け

GitHub Actionsの成果物 `Html2Apk-Windows-x64.zip` を展開し、`Html2Apk.exe` を起動します。

1. `index.html`を含むHTMLフォルダを選択
2. アプリ情報と必要な権限を設定
3. 出力先を選んで「APKをビルド」

初回のみ署名鍵の生成に少し時間がかかります。署名鍵は通常「ドキュメント/Html2Apk/keys」に保存されます。作成された`.h2akey`はアプリ更新に必須です。紛失すると同じパッケージIDの既存アプリを更新できません。

## JavaScript API

ローカルページには`window.H2A`が注入され、すべてPromiseを返します。

```js
await H2A.writeText("settings/user.json", JSON.stringify({ name: "Noa" }));
const text = await H2A.readText("settings/user.json");
const files = await H2A.list("settings");

await H2A.requestStorage();               // 選択モードの権限・SAFを要求
const songs = await H2A.listMedia("audio");

await H2A.writeText("saf:notes/a.txt", "hello");
await H2A.writeText("ext:Download/a.txt", "hello"); // 全ファイルモードのみ

await H2A.encrypt("settings/user.json", "passphrase");
await H2A.decrypt("settings/user.json", "passphrase");
```

API一覧は次の通りです。

- `readText`, `writeText`
- `readBase64`, `writeBase64`
- `exists`, `remove`, `mkdir`, `list`
- `encrypt`, `decrypt`
- `toUrl`, `listMedia`
- `requestStorage`, `requestCapability`

ストレージ許可結果は`h2astorage`、カメラ等の許可結果は`h2apermission`イベントで通知されます。

```js
window.addEventListener("h2astorage", event => console.log(event.detail));
await H2A.requestCapability("camera");
```


## 開発・ビルド

テンプレートAPKを先に作り、そのAPKをRustのEXEへ埋め込みます。ローカル開発にはRust、JDK 17、Android SDKが必要ですが、配布EXEの利用者には不要です。

```text
gradle -p template/android :app:assembleRelease :app:bundleRelease
copy template/android/app/build/outputs/apk/release/app-release-unsigned.apk template/template.apk
copy template/android/app/build/outputs/bundle/release/app-release.aab template/template.aab
cargo test
cargo build --release
```

Windows EXEは同梱のGitHub Actionsワークフローで自動生成できます。

## 設計上の注意

- Google Playへ提出する場合は出力形式にAAB（Google Play用）を選択してください。サイドロード配布にはAPKを使用します。
- `MANAGE_EXTERNAL_STORAGE`を使う全ファイルモードはGoogle Play審査対象が限定されます。通常はSAFを推奨します。
- APKテンプレートはtarget SDK更新のため定期的に再ビルドしてください。
- 外部Webページからネイティブブリッジは利用できません。
- 署名鍵ファイルは秘密情報です。安全な場所へバックアップしてください。

## ライセンス

本プロジェクトはMIT Licenseです。依存ライセンスはリリース時に`THIRD_PARTY_LICENSES.html`へ収録します。
