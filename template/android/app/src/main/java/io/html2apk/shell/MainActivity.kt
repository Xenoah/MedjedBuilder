package io.html2apk.shell

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.ActivityInfo
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.provider.MediaStore
import android.provider.Settings
import android.util.Base64
import android.webkit.GeolocationPermissions
import android.webkit.JavascriptInterface
import android.webkit.MimeTypeMap
import android.webkit.PermissionRequest
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.view.View
import android.view.WindowManager
import androidx.documentfile.provider.DocumentFile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.PBEKeySpec

class MainActivity : Activity() {
    lateinit var webView: WebView
        private set
    lateinit var config: JSONObject
        private set
    private var fileChooser: ValueCallback<Array<Uri>>? = null
    private var pendingCapability: String? = null

    // WebView.getUrl()はUIスレッド以外から呼べないため、ページURLをUIスレッドで
    // キャッシュしてブリッジ(JavaBridgeスレッド)から参照できるようにする
    @Volatile
    var currentPageUrl: String? = null
        private set

    private var statusBarColor = Color.parseColor("#202124")
    private var navigationBarColor = Color.BLACK

    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        config = assets.open("app.json").bufferedReader().use { JSONObject(it.readText()) }
        statusBarColor = runCatching { Color.parseColor(config.optString("status_bar_color")) }
            .getOrDefault(statusBarColor)
        navigationBarColor = runCatching { Color.parseColor(config.optString("navigation_bar_color")) }
            .getOrDefault(navigationBarColor)
        applyWindowOptions()
        webView = WebView(this)
        if (config.optBoolean("disable_splash")) {
            // 半透明テーマ利用時、ページ描画までWebViewの白背景が見えないようにする
            webView.setBackgroundColor(Color.TRANSPARENT)
        }
        setContentView(buildRootLayout())
        configureWebView()
        val page = config.optString("start_page", "index.html")
            .split('/').joinToString("/") { Uri.encode(it) }
        webView.loadUrl("file:///android_asset/www/$page")
    }

    /// Android 15+のエッジtoエッジ強制や半透明テーマでは、システムバーの背後に
    /// WebViewが素通しで描画されてバーが白く見える。バーと重なる領域を
    /// 設定色のパディングビューで埋め、WebView本体はバーの内側に収める。
    private fun buildRootLayout(): View {
        val root = android.widget.LinearLayout(this)
        root.orientation = android.widget.LinearLayout.VERTICAL
        val statusPad = View(this).apply { setBackgroundColor(statusBarColor) }
        val navPad = View(this).apply { setBackgroundColor(navigationBarColor) }
        val match = android.widget.LinearLayout.LayoutParams.MATCH_PARENT
        root.addView(statusPad, android.widget.LinearLayout.LayoutParams(match, 0))
        root.addView(webView, android.widget.LinearLayout.LayoutParams(match, 0, 1f))
        root.addView(navPad, android.widget.LinearLayout.LayoutParams(match, 0))
        root.setOnApplyWindowInsetsListener { _, insets ->
            val top: Int
            val bottom: Int
            val left: Int
            val right: Int
            if (Build.VERSION.SDK_INT >= 30) {
                val bars = insets.getInsets(
                    android.view.WindowInsets.Type.systemBars() or
                        android.view.WindowInsets.Type.displayCutout() or
                        android.view.WindowInsets.Type.ime()
                )
                top = bars.top; bottom = bars.bottom; left = bars.left; right = bars.right
            } else {
                @Suppress("DEPRECATION")
                run {
                    top = insets.systemWindowInsetTop
                    bottom = insets.systemWindowInsetBottom
                    left = insets.systemWindowInsetLeft
                    right = insets.systemWindowInsetRight
                }
            }
            statusPad.layoutParams = statusPad.layoutParams.apply { height = top }
            navPad.layoutParams = navPad.layoutParams.apply { height = bottom }
            root.setPadding(left, 0, right, 0)
            statusPad.requestLayout()
            navPad.requestLayout()
            insets
        }
        return root
    }

    private fun applyWindowOptions() {
        // 半透明テーマ(スプラッシュ非表示)ではAndroid 8.0/8.1が固定向きを
        // IllegalStateExceptionで拒否するため、失敗しても続行する
        runCatching {
            requestedOrientation = when (config.optString("orientation", "auto")) {
                "portrait" -> ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
                "landscape" -> ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
                else -> ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
            }
        }
        var flags = window.decorView.systemUiVisibility
        if (config.optBoolean("fullscreen")) {
            flags = flags or View.SYSTEM_UI_FLAG_FULLSCREEN or
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
        }
        // バー背景色の輝度に合わせてバーアイコンの明暗を切り替える
        // (暗色バーで黒アイコンのまま視認できなくなるのを防ぐ)
        flags = if (Color.luminance(statusBarColor) > 0.5f) {
            flags or View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
        } else {
            flags and View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR.inv()
        }
        if (Build.VERSION.SDK_INT >= 27) {
            flags = if (Color.luminance(navigationBarColor) > 0.5f) {
                flags or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
            } else {
                flags and View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR.inv()
            }
        }
        window.decorView.systemUiVisibility = flags
        if (config.optBoolean("keep_screen_on")) {
            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
        runCatching { window.statusBarColor = statusBarColor }
        runCatching { window.navigationBarColor = navigationBarColor }
    }

    @Suppress("SetJavaScriptEnabled")
    private fun configureWebView() {
        WebView.setWebContentsDebuggingEnabled(false)
        with(webView.settings) {
            javaScriptEnabled = true
            domStorageEnabled = true
            databaseEnabled = true
            allowFileAccess = true
            allowContentAccess = true
            allowFileAccessFromFileURLs = false
            allowUniversalAccessFromFileURLs = config.optBoolean("internet")
            mixedContentMode = if (config.optBoolean("allow_cleartext_http"))
                WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE else WebSettings.MIXED_CONTENT_NEVER_ALLOW
            mediaPlaybackRequiresUserGesture = false
        }
        if (config.optBoolean("file_api", true)) {
            webView.addJavascriptInterface(NativeBridge(this), "H2ANative")
        }
        webView.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                val uri = request.url
                if (uri.toString().startsWith("file:///android_asset/www/")) return false
                if ((uri.scheme == "http" || uri.scheme == "https") &&
                    config.optBoolean("open_external_links", true)) {
                    startActivity(Intent(Intent.ACTION_VIEW, uri))
                    return true
                }
                return false
            }

            override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
                currentPageUrl = url
            }

            override fun doUpdateVisitedHistory(view: WebView, url: String, isReload: Boolean) {
                currentPageUrl = url
            }

            override fun onPageFinished(view: WebView, url: String) {
                currentPageUrl = url
                if (url.startsWith("file:///android_asset/www/")) view.evaluateJavascript(BRIDGE_JS, null)
            }
        }
        webView.webChromeClient = object : WebChromeClient() {
            override fun onShowFileChooser(
                webView: WebView,
                callback: ValueCallback<Array<Uri>>,
                params: WebChromeClient.FileChooserParams
            ): Boolean {
                fileChooser?.onReceiveValue(null)
                fileChooser = callback
                return runCatching {
                    startActivityForResult(params.createIntent(), REQUEST_FILE)
                    true
                }.getOrElse {
                    fileChooser = null
                    false
                }
            }

            override fun onPermissionRequest(request: PermissionRequest) {
                runOnUiThread {
                    val allowed = request.resources.filter {
                        when (it) {
                            PermissionRequest.RESOURCE_VIDEO_CAPTURE -> capabilityGranted("camera")
                            PermissionRequest.RESOURCE_AUDIO_CAPTURE -> capabilityGranted("microphone")
                            else -> false
                        }
                    }.toTypedArray()
                    if (allowed.isEmpty()) request.deny() else request.grant(allowed)
                }
            }

            override fun onGeolocationPermissionsShowPrompt(
                origin: String,
                callback: GeolocationPermissions.Callback
            ) {
                callback.invoke(origin, capabilityGranted("location"), false)
            }
        }
    }

    private fun capabilityGranted(name: String): Boolean {
        if (!config.optBoolean(name)) return false
        val permission = when (name) {
            "camera" -> Manifest.permission.CAMERA
            "microphone" -> Manifest.permission.RECORD_AUDIO
            "location" -> Manifest.permission.ACCESS_FINE_LOCATION
            else -> return false
        }
        return checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED
    }

    fun requestCapability(name: String) {
        val permission = when (name) {
            "camera" -> Manifest.permission.CAMERA
            "microphone" -> Manifest.permission.RECORD_AUDIO
            "location" -> Manifest.permission.ACCESS_FINE_LOCATION
            "notifications" -> if (Build.VERSION.SDK_INT >= 33) Manifest.permission.POST_NOTIFICATIONS else null
            else -> null
        }
        if (permission == null || !config.optBoolean(name)) {
            dispatch("h2apermission", JSONObject().put("name", name).put("granted", permission == null))
            return
        }
        pendingCapability = name
        requestPermissions(arrayOf(permission), REQUEST_CAPABILITY)
    }

    fun requestStorage() {
        when (config.optString("storage", "private")) {
            "saf" -> startActivityForResult(
                Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).addFlags(
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                        Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
                ), REQUEST_TREE
            )
            "media" -> {
                val permissions = if (Build.VERSION.SDK_INT >= 33) arrayOf(
                    Manifest.permission.READ_MEDIA_AUDIO,
                    Manifest.permission.READ_MEDIA_IMAGES,
                    Manifest.permission.READ_MEDIA_VIDEO
                ) else arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE)
                requestPermissions(permissions, REQUEST_STORAGE)
            }
            "all_files" -> {
                if (Build.VERSION.SDK_INT >= 30 && !Environment.isExternalStorageManager()) {
                    startActivityForResult(
                        Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                            Uri.parse("package:$packageName")), REQUEST_ALL_FILES
                    )
                } else if (Build.VERSION.SDK_INT < 30) {
                    requestPermissions(arrayOf(
                        Manifest.permission.READ_EXTERNAL_STORAGE,
                        Manifest.permission.WRITE_EXTERNAL_STORAGE
                    ), REQUEST_STORAGE)
                } else dispatchStorage(true)
            }
            else -> dispatchStorage(true)
        }
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        when (requestCode) {
            REQUEST_FILE -> {
                val result = WebChromeClient.FileChooserParams.parseResult(resultCode, data)
                fileChooser?.onReceiveValue(result)
                fileChooser = null
            }
            REQUEST_TREE -> {
                val uri = data?.data
                if (resultCode == RESULT_OK && uri != null) {
                    val flags = (data?.flags ?: 0) and (Intent.FLAG_GRANT_READ_URI_PERMISSION or
                        Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
                    contentResolver.takePersistableUriPermission(uri, flags)
                    getPreferences(MODE_PRIVATE).edit().putString("tree_uri", uri.toString()).apply()
                    dispatchStorage(true)
                } else dispatchStorage(false)
            }
            REQUEST_ALL_FILES -> dispatchStorage(Build.VERSION.SDK_INT < 30 || Environment.isExternalStorageManager())
        }
    }

    override fun onRequestPermissionsResult(code: Int, permissions: Array<out String>, results: IntArray) {
        super.onRequestPermissionsResult(code, permissions, results)
        val granted = results.isNotEmpty() && results.all { it == PackageManager.PERMISSION_GRANTED }
        if (code == REQUEST_STORAGE) dispatchStorage(granted)
        if (code == REQUEST_CAPABILITY) {
            dispatch("h2apermission", JSONObject().put("name", pendingCapability)
                .put("granted", granted))
            pendingCapability = null
        }
    }

    private fun dispatchStorage(granted: Boolean) {
        dispatch("h2astorage", JSONObject().put("granted", granted)
            .put("mode", config.optString("storage", "private")))
    }

    private fun dispatch(name: String, detail: JSONObject) {
        runOnUiThread {
            webView.evaluateJavascript(
                "window.dispatchEvent(new CustomEvent(${JSONObject.quote(name)},{detail:${detail}}));",
                null
            )
        }
    }

    override fun onBackPressed() {
        if (webView.canGoBack()) webView.goBack() else super.onBackPressed()
    }

    companion object {
        private const val REQUEST_FILE = 5101
        private const val REQUEST_TREE = 5102
        private const val REQUEST_STORAGE = 5103
        private const val REQUEST_ALL_FILES = 5104
        private const val REQUEST_CAPABILITY = 5105
        private val BRIDGE_JS = """
            (() => {
              if (window.H2A) return;
              const call = (method, args=[]) => new Promise((resolve, reject) => {
                try {
                  const result = JSON.parse(H2ANative.call(method, JSON.stringify(args)));
                  result.ok ? resolve(result.value) : reject(new Error(result.error));
                } catch (e) { reject(e); }
              });
              const names = ['readText','writeText','readBase64','writeBase64','openRead','readChunk',
                'closeRead','openRandom','readRandom','closeRandom','extractZipStart','extractZipStatus',
                'exists','remove','mkdir','list','encrypt','decrypt','toUrl','listMedia',
                'requestStorage','requestCapability'];
              window.H2A = { call };
              names.forEach(name => window.H2A[name] = (...args) => call(name, args));
            })();
        """.trimIndent()
    }
}

private class NativeBridge(private val activity: MainActivity) {
    @JavascriptInterface
    fun call(method: String, arguments: String): String {
        // このメソッドはJavaBridgeスレッドで実行されるため、WebView.getUrl()は使えない
        // (UIスレッド外から呼ぶとRuntimeExceptionになる)。キャッシュしたURLで判定する。
        if (activity.currentPageUrl?.startsWith("file:///android_asset/www/") != true) {
            return failure("このページからはネイティブAPIを使用できません")
        }
        return runCatching {
            val args = JSONArray(arguments)
            when (method) {
                "readText" -> success(String(readBytes(args.getString(0)), Charsets.UTF_8))
                "writeText" -> { writeBytes(args.getString(0), args.getString(1).toByteArray()); success(true) }
                "readBase64" -> success(Base64.encodeToString(readBytes(args.getString(0)), Base64.NO_WRAP))
                "writeBase64" -> { writeBytes(args.getString(0), Base64.decode(args.getString(1), Base64.DEFAULT)); success(true) }
                "openRead" -> success(openRead(args.getString(0)))
                "readChunk" -> base64Success(readChunk(args.getInt(0), args.getInt(1)))
                "closeRead" -> { closeRead(args.getInt(0)); success(true) }
                "extractZipStart" -> success(extractZipStart(args.getString(0), args.getString(1)))
                "extractZipStatus" -> success(extractZipStatus(args.getInt(0)))
                "openRandom" -> success(openRandom(args.getString(0)))
                "readRandom" -> base64Success(readRandom(args.getInt(0), args.getLong(1), args.getInt(2)))
                "closeRandom" -> { closeRandom(args.getInt(0)); success(true) }
                "exists" -> success(exists(args.getString(0)))
                "remove" -> success(remove(args.getString(0)))
                "mkdir" -> success(mkdir(args.getString(0)))
                "list" -> success(list(args.getString(0)))
                "encrypt" -> { cryptFile(args.getString(0), args.getString(1), true); success(args.getString(0)) }
                "decrypt" -> { cryptFile(args.getString(0), args.getString(1), false); success(args.getString(0)) }
                "toUrl" -> success(toUrl(args.getString(0)))
                "listMedia" -> success(listMedia(args.optString(0, "audio")))
                "requestStorage" -> { activity.runOnUiThread { activity.requestStorage() }; success(true) }
                "requestCapability" -> {
                    activity.runOnUiThread { activity.requestCapability(args.getString(0)) }; success(true)
                }
                else -> failure("不明なAPI: $method")
            }
        }.getOrElse { failure(it.message ?: it.javaClass.simpleName) }
    }

    private fun readBytes(path: String): ByteArray {
        if (path.startsWith("saf:")) {
            val doc = safDocument(path.removePrefix("saf:"), false, false)
                ?: error("ファイルが見つかりません")
            return activity.contentResolver.openInputStream(doc.uri)!!.use { it.readBytes() }
        }
        return resolveFile(path).readBytes()
    }

    // 大きなファイルをJavaヒープに収まるサイズずつ読むためのストリームAPI。
    // openRead でIDを取得し、readChunk をEOF(空文字)まで繰り返し、closeRead で解放する。
    private val readStreams = java.util.concurrent.ConcurrentHashMap<Int, java.io.InputStream>()
    private val nextReadId = java.util.concurrent.atomic.AtomicInteger(1)

    private fun openStream(path: String): java.io.InputStream {
        if (path.startsWith("saf:")) {
            val doc = safDocument(path.removePrefix("saf:"), false, false)
                ?: error("ファイルが見つかりません")
            return activity.contentResolver.openInputStream(doc.uri) ?: error("ファイルを開けません")
        }
        return resolveFile(path).inputStream()
    }

    private fun openRead(path: String): Int {
        val id = nextReadId.getAndIncrement()
        readStreams[id] = openStream(path)
        return id
    }

    private fun readChunk(id: Int, length: Int): String {
        if (length <= 0 || length > 16 * 1024 * 1024) error("チャンクサイズが不正です")
        val stream = readStreams[id] ?: error("ストリームが開かれていません")
        val buf = ByteArray(length)
        var read = 0
        while (read < length) {
            val r = stream.read(buf, read, length - read)
            if (r < 0) break
            read += r
        }
        return if (read <= 0) "" else Base64.encodeToString(buf, 0, read, Base64.NO_WRAP)
    }

    private fun closeRead(id: Int) {
        readStreams.remove(id)?.close()
    }

    // 任意の位置から読めるランダムアクセスAPI。ZIPアーカイブのように
    // 「末尾の目次だけ読む」「必要なエントリだけ読む」用途で、
    // ファイル全体を転送せずに済む。openRandom は {id, size} を返す。
    private class RandomSource(val pfd: android.os.ParcelFileDescriptor?, val stream: java.io.FileInputStream) {
        val channel: java.nio.channels.FileChannel = stream.channel
    }

    private val randomSources = java.util.concurrent.ConcurrentHashMap<Int, RandomSource>()

    private fun openRandom(path: String): JSONObject {
        val source = if (path.startsWith("saf:")) {
            val doc = safDocument(path.removePrefix("saf:"), false, false) ?: error("ファイルが見つかりません")
            val pfd = activity.contentResolver.openFileDescriptor(doc.uri, "r") ?: error("ファイルを開けません")
            RandomSource(pfd, java.io.FileInputStream(pfd.fileDescriptor))
        } else {
            RandomSource(null, java.io.FileInputStream(resolveFile(path)))
        }
        val id = nextReadId.getAndIncrement()
        randomSources[id] = source
        return JSONObject().put("id", id).put("size", source.channel.size())
    }

    private fun readRandom(id: Int, offset: Long, length: Int): String {
        if (length <= 0 || length > 16 * 1024 * 1024) error("チャンクサイズが不正です")
        val source = randomSources[id] ?: error("ストリームが開かれていません")
        val buf = java.nio.ByteBuffer.allocate(length)
        var pos = offset
        while (buf.hasRemaining()) {
            val r = source.channel.read(buf, pos)
            if (r < 0) break
            pos += r
        }
        val read = buf.position()
        return if (read <= 0) "" else Base64.encodeToString(buf.array(), 0, read, Base64.NO_WRAP)
    }

    private fun closeRandom(id: Int) {
        randomSources.remove(id)?.let {
            runCatching { it.channel.close() }
            runCatching { it.stream.close() }
            runCatching { it.pfd?.close() }
        }
    }

    // ZIPをアプリ専用領域(filesDir/h2a配下)へネイティブスレッドで展開する。
    // Base64でJSへ転送するよりはるかに速く、展開後は toUrl でファイルを直接表示できる。
    // extractZipStart でジョブIDを取得し、extractZipStatus を done になるまでポーリングする。
    private class ExtractJob {
        @Volatile var done = false
        @Volatile var count = 0
        @Volatile var error: String? = null
    }

    private val extractJobs = java.util.concurrent.ConcurrentHashMap<Int, ExtractJob>()

    private fun extractZipStart(srcPath: String, destDir: String): Int {
        if (destDir.startsWith("saf:") || destDir.startsWith("ext:") || destDir.startsWith("file:")) {
            error("展開先はアプリ専用領域のみ指定できます")
        }
        val dest = resolveFile(destDir)
        val id = nextReadId.getAndIncrement()
        val job = ExtractJob()
        extractJobs[id] = job
        Thread {
            try {
                if (dest.exists()) dest.deleteRecursively()
                dest.mkdirs()
                val charset = try { charset("Shift_JIS") } catch (e: Exception) { Charsets.UTF_8 }
                java.util.zip.ZipInputStream(openStream(srcPath).buffered(), charset).use { zin ->
                    while (true) {
                        val entry = zin.nextEntry ?: break
                        if (entry.isDirectory) continue
                        val name = entry.name.replace('\\', '/')
                        val out = File(dest, name)
                        // zip-slip対策: 展開先の外に出るエントリは無視する
                        if (!out.canonicalPath.startsWith(dest.canonicalPath + File.separator)) continue
                        out.parentFile?.mkdirs()
                        out.outputStream().use { zin.copyTo(it) }
                        job.count++
                    }
                }
            } catch (e: Exception) {
                job.error = e.message ?: e.javaClass.simpleName
            }
            job.done = true
        }.start()
        return id
    }

    private fun extractZipStatus(id: Int): JSONObject {
        val job = extractJobs[id] ?: error("展開ジョブが見つかりません")
        val result = JSONObject().put("done", job.done).put("count", job.count)
        job.error?.let { result.put("error", it) }
        if (job.done) extractJobs.remove(id)
        return result
    }

    private fun writeBytes(path: String, bytes: ByteArray) {
        if (path.startsWith("saf:")) {
            val doc = safDocument(path.removePrefix("saf:"), true, true) ?: error("ファイルを作成できません")
            activity.contentResolver.openOutputStream(doc.uri, "wt")!!.use { it.write(bytes) }
            return
        }
        val file = resolveFile(path)
        file.parentFile?.mkdirs()
        file.writeBytes(bytes)
    }

    private fun exists(path: String): Boolean = if (path.startsWith("saf:"))
        safDocument(path.removePrefix("saf:"), false, false)?.exists() == true else resolveFile(path).exists()

    private fun remove(path: String): Boolean = if (path.startsWith("saf:"))
        safDocument(path.removePrefix("saf:"), false, false)?.delete() == true else resolveFile(path).deleteRecursively()

    private fun mkdir(path: String): Boolean {
        if (path.startsWith("saf:")) return safDocument(path.removePrefix("saf:"), true, false)?.isDirectory == true
        return resolveFile(path).mkdirs()
    }

    private fun list(path: String): JSONArray {
        val result = JSONArray()
        if (path.startsWith("saf:")) {
            val directory = safDocument(path.removePrefix("saf:"), false, false) ?: return result
            directory.listFiles().sortedBy { it.name }.forEach { doc ->
                result.put(JSONObject().put("name", doc.name).put("directory", doc.isDirectory)
                    .put("size", doc.length()).put("modified", doc.lastModified()).put("uri", doc.uri.toString()))
            }
        } else {
            resolveFile(path).listFiles()?.sortedBy { it.name }?.forEach { file ->
                result.put(JSONObject().put("name", file.name).put("directory", file.isDirectory)
                    .put("size", file.length()).put("modified", file.lastModified()))
            }
        }
        return result
    }

    private fun resolveFile(path: String): File {
        val fileUri = path.startsWith("file:")
        val external = path.startsWith("ext:") || fileUri
        if (external && activity.config.optString("storage") != "all_files") error("全ファイルモードが必要です")
        if (external && Build.VERSION.SDK_INT >= 30 && !Environment.isExternalStorageManager()) error("全ファイル権限がありません")
        val relative = if (path.startsWith("ext:")) path.removePrefix("ext:") else path
        if (relative.contains('\u0000')) error("不正なパスです")
        val base = (if (external) Environment.getExternalStorageDirectory()
            else File(activity.filesDir, "h2a")).canonicalFile
        base.mkdirs()
        val target = if (fileUri) File(Uri.parse(path).path ?: error("不正なURIです")).canonicalFile
            else File(base, relative.trimStart('/', '\\')).canonicalFile
        if (target != base && !target.path.startsWith(base.path + File.separator)) error("フォルダ外にはアクセスできません")
        return target
    }

    private fun treeRoot(): DocumentFile {
        val value = activity.getPreferences(Activity.MODE_PRIVATE).getString("tree_uri", null)
            ?: error("フォルダが選択されていません")
        return DocumentFile.fromTreeUri(activity, Uri.parse(value)) ?: error("選択フォルダを開けません")
    }

    private fun safDocument(path: String, create: Boolean, fileAtEnd: Boolean): DocumentFile? {
        val parts = path.replace('\\', '/').split('/').filter { it.isNotBlank() && it != "." }
        if (parts.any { it == ".." }) error("フォルダ外にはアクセスできません")
        var current = treeRoot()
        parts.forEachIndexed { index, name ->
            val last = index == parts.lastIndex
            val found = current.findFile(name)
            current = found ?: if (!create) return null else if (last && fileAtEnd) {
                current.createFile(mime(name), name) ?: return null
            } else current.createDirectory(name) ?: return null
        }
        return current
    }

    private fun mime(name: String): String = MimeTypeMap.getSingleton()
        .getMimeTypeFromExtension(name.substringAfterLast('.', "")) ?: "application/octet-stream"

    private fun toUrl(path: String): String = when {
        path.startsWith("saf:") -> safDocument(path.removePrefix("saf:"), false, false)?.uri?.toString()
            ?: error("ファイルが見つかりません")
        else -> Uri.fromFile(resolveFile(path)).toString()
    }

    private fun listMedia(kind: String): JSONArray {
        if (activity.config.optString("storage") !in setOf("media", "all_files")) error("メディア権限が無効です")
        val (uri, projection) = when (kind) {
            "image" -> MediaStore.Images.Media.EXTERNAL_CONTENT_URI to arrayOf(
                MediaStore.Images.Media._ID, MediaStore.Images.Media.DISPLAY_NAME, MediaStore.Images.Media.SIZE)
            "video" -> MediaStore.Video.Media.EXTERNAL_CONTENT_URI to arrayOf(
                MediaStore.Video.Media._ID, MediaStore.Video.Media.DISPLAY_NAME, MediaStore.Video.Media.SIZE,
                MediaStore.Video.Media.DURATION)
            else -> MediaStore.Audio.Media.EXTERNAL_CONTENT_URI to arrayOf(
                MediaStore.Audio.Media._ID, MediaStore.Audio.Media.TITLE, MediaStore.Audio.Media.ARTIST,
                MediaStore.Audio.Media.SIZE, MediaStore.Audio.Media.DURATION)
        }
        val result = JSONArray()
        activity.contentResolver.query(uri, projection, null, null, null)?.use { cursor ->
            while (cursor.moveToNext()) {
                val id = cursor.getLong(0)
                val item = JSONObject().put("id", id).put("title", cursor.getString(1) ?: "")
                    .put("url", Uri.withAppendedPath(uri, id.toString()).toString())
                for (i in 2 until projection.size) item.put(projection[i].substringAfterLast('_').lowercase(), cursor.getString(i))
                result.put(item)
            }
        }
        return result
    }

    private fun cryptFile(path: String, password: String, encrypt: Boolean) {
        val input = readBytes(path)
        val output = if (encrypt) encrypt(input, password) else decrypt(input, password)
        writeBytes(path, output)
    }

    private fun encrypt(data: ByteArray, password: String): ByteArray {
        val salt = ByteArray(16).also(SecureRandom()::nextBytes)
        val nonce = ByteArray(12).also(SecureRandom()::nextBytes)
        val key = derive(password, salt)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key, GCMParameterSpec(128, nonce))
        return "H2AE1".toByteArray() + salt + nonce + cipher.doFinal(data)
    }

    private fun decrypt(data: ByteArray, password: String): ByteArray {
        if (data.size < 33 || String(data.copyOfRange(0, 5)) != "H2AE1") error("暗号化ファイル形式が不正です")
        val salt = data.copyOfRange(5, 21)
        val nonce = data.copyOfRange(21, 33)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, derive(password, salt), GCMParameterSpec(128, nonce))
        return cipher.doFinal(data.copyOfRange(33, data.size))
    }

    private fun derive(password: String, salt: ByteArray) = javax.crypto.spec.SecretKeySpec(
        SecretKeyFactory.getInstance("PBKDF2WithHmacSHA256")
            .generateSecret(PBEKeySpec(password.toCharArray(), salt, 150_000, 256)).encoded, "AES")

    // Base64はJSONエスケープ不要な文字のみなので、数MB級の応答は
    // JSONObject.quote の1文字ずつのエスケープ処理を通さず直接組み立てる(高速)
    private fun base64Success(value: String): String = "{\"ok\":true,\"value\":\"$value\"}"

    private fun success(value: Any?): String = JSONObject().put("ok", true)
        .put("value", value ?: JSONObject.NULL).toString()
    private fun failure(message: String): String = JSONObject().put("ok", false).put("error", message).toString()
}
