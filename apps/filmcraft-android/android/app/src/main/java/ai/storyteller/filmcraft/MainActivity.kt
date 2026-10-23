package ai.storyteller.filmcraft

import android.Manifest
import android.content.ContentValues
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Bundle
import android.os.FileObserver
import android.os.Handler
import android.os.Looper
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.view.WindowManager
import android.webkit.MimeTypeMap
import android.widget.Toast
import com.google.androidgamesdk.GameActivity
import java.io.File
import kotlin.concurrent.thread

/**
 * Hosts the Rust app (libfilmcraft_android.so). Rust calls [showDialog] and [openUrl]; answers go
 * back through [nativeOnResult].
 *
 * Files: imports are copied from the picked document into filesDir/Imports (the engine works on
 * real paths). Saves return a path in app storage immediately; finished exports in the Exports
 * folder are then published to Movies/Music/Pictures ▸ FilmCraft through MediaStore.
 */
class MainActivity : GameActivity() {
    companion object {
        init { System.loadLibrary("filmcraft_android") }
        private const val REQ_OPEN = 4101
        private const val REQ_MIC = 4102
    }

    private external fun nativeOnResult(requestId: Int, paths: Array<String>)

    private var pendingId = -1
    private val main = Handler(Looper.getMainLooper())
    private val publishTimers = HashMap<String, Runnable>()
    private val published = HashSet<String>()
    private var observer: FileObserver? = null

    private val exportsDir: File get() = (getExternalFilesDir(null) ?: filesDir).let { File(it, "Exports") }.also { it.mkdirs() }
    private val projectsDir: File get() = File(filesDir, "Projects").also { it.mkdirs() }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) // no screen timeout mid-edit
        watchExports()
        val prefs = getSharedPreferences("filmcraft", MODE_PRIVATE)
        if (!prefs.getBoolean("mic_asked", false)) { // voice-over recording needs the microphone
            prefs.edit().putBoolean("mic_asked", true).apply()
            if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
                requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), REQ_MIC)
            }
        }
    }

    // ---- called from Rust ---------------------------------------------------------------

    @Suppress("unused")
    fun showDialog(requestId: Int, kind: String, exts: Array<String>, name: String) {
        runOnUiThread {
            when (kind) {
                "open", "openMany" -> {
                    pendingId = requestId
                    val mimes = mimeTypes(exts)
                    val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                        addCategory(Intent.CATEGORY_OPENABLE)
                        type = "*/*"
                        if (mimes.isNotEmpty()) putExtra(Intent.EXTRA_MIME_TYPES, mimes)
                        putExtra(Intent.EXTRA_ALLOW_MULTIPLE, kind == "openMany")
                    }
                    try { startActivityForResult(intent, REQ_OPEN) } catch (e: Exception) { nativeOnResult(requestId, emptyArray()) }
                }
                "save" -> {
                    val project = exts.any { it.equals("fcproj", true) }
                    val dir = if (project) projectsDir else exportsDir
                    val base = File(name).name.ifBlank { if (project) "Untitled.fcproj" else "Export" }
                    val target = if (project) File(dir, base) else uniqueFile(dir, base)
                    nativeOnResult(requestId, arrayOf(target.absolutePath))
                }
                else -> nativeOnResult(requestId, arrayOf(exportsDir.absolutePath)) // "folder"
            }
        }
    }

    @Suppress("unused")
    fun openUrl(url: String) {
        runOnUiThread {
            try { startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) } catch (_: Exception) {}
        }
    }

    // ---- picker result ------------------------------------------------------------------

    @Deprecated("GameActivity is not a ComponentActivity; the classic result API is the simplest here")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != REQ_OPEN) return
        val id = pendingId
        pendingId = -1
        val uris = ArrayList<Uri>()
        if (resultCode == RESULT_OK && data != null) {
            data.clipData?.let { c -> for (i in 0 until c.itemCount) uris.add(c.getItemAt(i).uri) }
            if (uris.isEmpty()) data.data?.let { uris.add(it) }
        }
        if (uris.isEmpty()) { nativeOnResult(id, emptyArray()); return }
        Toast.makeText(this, "Importing…", Toast.LENGTH_SHORT).show()
        thread(name = "import") { nativeOnResult(id, uris.mapNotNull { copyIn(it) }.toTypedArray()) }
    }

    private fun copyIn(uri: Uri): String? = try {
        var display = "media"
        var size = -1L
        contentResolver.query(uri, null, null, null, null)?.use { c ->
            if (c.moveToFirst()) {
                c.getColumnIndex(OpenableColumns.DISPLAY_NAME).takeIf { it >= 0 }?.let { display = c.getString(it) }
                c.getColumnIndex(OpenableColumns.SIZE).takeIf { it >= 0 }?.let { size = c.getLong(it) }
            }
        }
        // one folder per source document: importing the same file twice reuses the copy
        val dir = File(filesDir, "Imports/" + Integer.toHexString(uri.toString().hashCode())).also { it.mkdirs() }
        val out = File(dir, display)
        if (!(out.exists() && size >= 0 && out.length() == size)) {
            contentResolver.openInputStream(uri)?.use { i -> out.outputStream().use { o -> i.copyTo(o, 1 shl 20) } }
        }
        out.absolutePath
    } catch (e: Exception) { null }

    private fun mimeTypes(exts: Array<String>): Array<String> {
        val map = MimeTypeMap.getSingleton()
        val mimes = exts.map { map.getMimeTypeFromExtension(it.lowercase()) }
        return if (mimes.any { it == null }) emptyArray() else mimes.filterNotNull().distinct().toTypedArray()
    }

    private fun uniqueFile(dir: File, name: String): File {
        var f = File(dir, name)
        val stem = f.nameWithoutExtension; val ext = f.extension.let { if (it.isEmpty()) "" else ".$it" }
        var n = 1
        while (f.exists()) f = File(dir, "$stem ($n)$ext").also { n++ }
        return f
    }

    // ---- publish finished exports to the gallery ----------------------------------------

    private fun watchExports() {
        val dir = exportsDir
        observer = object : FileObserver(dir, CLOSE_WRITE or MOVED_TO) {
            override fun onEvent(event: Int, path: String?) {
                if (path == null) return
                val file = File(dir, path)
                // an exporter may close and reopen the file (fast-start rewrite): publish once it is quiet
                main.post {
                    publishTimers.remove(path)?.let { main.removeCallbacks(it) }
                    val r = Runnable { publish(file) }
                    publishTimers[path] = r
                    main.postDelayed(r, 3000)
                }
            }
        }.also { it.startWatching() }
    }

    private fun publish(file: File) {
        if (!file.exists() || file.length() == 0L) return
        val key = file.name + file.lastModified()
        if (!published.add(key)) return
        val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(file.extension.lowercase()) ?: return
        val (collection, folder) = when {
            mime.startsWith("video/") -> MediaStore.Video.Media.EXTERNAL_CONTENT_URI to "Movies/FilmCraft"
            mime.startsWith("audio/") -> MediaStore.Audio.Media.EXTERNAL_CONTENT_URI to "Music/FilmCraft"
            mime.startsWith("image/") -> MediaStore.Images.Media.EXTERNAL_CONTENT_URI to "Pictures/FilmCraft"
            else -> return
        }
        thread(name = "publish") {
            try {
                val values = ContentValues().apply {
                    put(MediaStore.MediaColumns.DISPLAY_NAME, file.name)
                    put(MediaStore.MediaColumns.MIME_TYPE, mime)
                    put(MediaStore.MediaColumns.RELATIVE_PATH, folder)
                    put(MediaStore.MediaColumns.IS_PENDING, 1)
                }
                val uri = contentResolver.insert(collection, values) ?: return@thread
                contentResolver.openOutputStream(uri)?.use { o -> file.inputStream().use { it.copyTo(o, 1 shl 20) } }
                values.clear(); values.put(MediaStore.MediaColumns.IS_PENDING, 0)
                contentResolver.update(uri, values, null, null)
                main.post { Toast.makeText(this, "Saved to $folder", Toast.LENGTH_LONG).show() }
            } catch (e: Exception) {
                published.remove(key)
            }
        }
    }
}
