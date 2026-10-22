package no.cloudjumper.app

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.os.StatFs
import android.os.storage.StorageManager
import android.provider.DocumentsContract
import android.provider.Settings
import android.webkit.JavascriptInterface
import android.view.WindowManager
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

class MainActivity : TauriActivity() {
  private var webView: WebView? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Kopiering av mange bilder tar tid. Skjermen holdes på så telefonen ikke stopper appen underveis.
    window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    // Hold appen unna statuslinjen og knappene nederst, ellers havner tittel og knapper under dem.
    ViewCompat.setOnApplyWindowInsetsListener(findViewById(android.R.id.content)) { v, innfelt ->
      val k = innfelt.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.ime())
      v.setPadding(k.left, k.top, k.right, k.bottom)
      WindowInsetsCompat.CONSUMED
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    webView.addJavascriptInterface(Telefon(), "CloudJumperAndroid")
  }

  // Når brukeren kommer tilbake fra innstillingene, får grensesnittet sjekke tilgangen på nytt.
  override fun onResume() {
    super.onResume()
    webView?.evaluateJavascript("window.dispatchEvent(new Event('cloudjumper-tilbake'))", null)
  }

  /** Det grensesnittet (mobil.js) kan spørre telefonen om. Sletter aldri noe. */
  inner class Telefon {
    @JavascriptInterface
    fun harTilgang(): Boolean =
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) Environment.isExternalStorageManager()
      else checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE) == PackageManager.PERMISSION_GRANTED

    @JavascriptInterface
    fun beOmTilgang() = runOnUiThread {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        try {
          startActivity(Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:$packageName")))
        } catch (e: Exception) {
          startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION))
        }
      } else {
        requestPermissions(
          arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE, Manifest.permission.WRITE_EXTERNAL_STORAGE)
        ) { webView?.evaluateJavascript("window.dispatchEvent(new Event('cloudjumper-tilbake'))", null) }
      }
    }

    /** Telefonen selv, minnekort og USB-disker, med navnet Android gir dem og ledig plass. */
    @JavascriptInterface
    fun lagringsplasser(): String {
      val ut = JSONArray()
      val sm = getSystemService(StorageManager::class.java)
      for (v in sm.storageVolumes) {
        if (v.state != Environment.MEDIA_MOUNTED) continue
        val rot = mappeFor(v) ?: continue
        val ledig = try { StatFs(rot.path).availableBytes } catch (e: Exception) { -1L }
        ut.put(JSONObject().apply {
          put("navn", v.getDescription(this@MainActivity))
          put("sti", rot.path)
          put("ekstern", v.isRemovable)
          put("ledig", ledig)
        })
      }
      return ut.toString()
    }

    /** Åpner mappen i telefonens filbehandler, så brukeren kan se de sorterte bildene. */
    @JavascriptInterface
    fun apneMappe(sti: String) = runOnUiThread {
      val uri = dokumentUri(File(sti))
      val forsok = listOfNotNull(
        uri?.let { Intent(Intent.ACTION_VIEW).setDataAndType(it, DocumentsContract.Document.MIME_TYPE_DIR) },
        uri?.let { Intent(Intent.ACTION_VIEW, it) },
        Intent(Intent.ACTION_VIEW).setType(DocumentsContract.Root.MIME_TYPE_ITEM),
      )
      for (i in forsok) {
        try {
          startActivity(i.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION))
          return@runOnUiThread
        } catch (e: Exception) {}
      }
    }
  }

  private fun mappeFor(v: android.os.storage.StorageVolume): File? {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) return v.directory
    if (v.isPrimary) return Environment.getExternalStorageDirectory()
    return v.uuid?.let { File("/storage/$it") }?.takeIf { it.exists() }
  }

  /** content://-adressen filbehandleren bruker for en mappe, for eksempel primary:Pictures/CloudJumper. */
  private fun dokumentUri(mappe: File): Uri? {
    val sm = getSystemService(StorageManager::class.java)
    val v = sm.getStorageVolume(mappe) ?: return null
    val rot = mappeFor(v) ?: return null
    val id = if (v.isPrimary) "primary" else v.uuid ?: return null
    val rest = mappe.path.removePrefix(rot.path).trimStart('/')
    return DocumentsContract.buildDocumentUri("com.android.externalstorage.documents", "$id:$rest")
  }
}
