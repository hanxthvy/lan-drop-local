package id.hanxthvy.landrop

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import java.io.File

class MainActivity : TauriActivity() {
  private var multicastLock: WifiManager.MulticastLock? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()

    // Folder simpan harus lewat Context, bukan path karangan Rust: sejak
    // Android 10 aplikasi tidak boleh membuat /storage/emulated/0/Android/data
    // sendiri, tapi getExternalFilesDir dilayani FUSE dan selalu bisa ditulis.
    // Path-nya dicatat sebelum super.onCreate, karena di situlah Rust menyala
    // dan membaca penanda ini.
    val recorded =
      runCatching {
        val dir = getExternalFilesDir("Download")
        File(filesDir, SAVE_DIR_MARKER).writeText(dir?.absolutePath.orEmpty())
        dir?.absolutePath.orEmpty()
      }.getOrElse { "" }
    android.util.Log.i("LanDrop", "folder simpan=$recorded")

    super.onCreate(savedInstanceState)

    // Android 15 memaksa edge-to-edge untuk targetSdk 35+, jadi UI tetap
    // menggambar di bawah status bar kalau tidak diberi inset. Padding pada
    // root view menggeser seluruh WebView ke bawah status bar dan di atas
    // navigation bar — UI di public/ tidak perlu diubah.
    val root = findViewById<android.view.View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(root) { view, insets ->
      val bars =
        insets.getInsets(
          WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
        )
      view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
      insets
    }
    ViewCompat.requestApplyInsets(root)

    // Discovery memakai UDP broadcast ke port 41234. Tanpa lock ini Android
    // membuang paket broadcast begitu layar mati atau Wi-Fi masuk mode hemat
    // daya, jadi perangkat lain tidak pernah melihat HP ini.
    val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
    multicastLock =
      wifi?.createMulticastLock("landrop-discovery")?.apply {
        setReferenceCounted(true)
        runCatching { acquire() }
          .onFailure { android.util.Log.w("LanDrop", "multicast lock gagal: $it") }
      }
  }

  override fun onDestroy() {
    runCatching { multicastLock?.release() }
    multicastLock = null
    super.onDestroy()
  }

  companion object {
    const val SAVE_DIR_MARKER = "landrop-save-dir"
  }
}
