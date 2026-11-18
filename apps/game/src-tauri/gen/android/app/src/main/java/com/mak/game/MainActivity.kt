package com.mak.game

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    val content = findViewById<android.view.View>(android.R.id.content)
    val left = content.paddingLeft
    val top = content.paddingTop
    val right = content.paddingRight
    val bottom = content.paddingBottom
    val safeArea = WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(safeArea)
      view.setPadding(left + bars.left, top + bars.top, right + bars.right, bottom + bars.bottom)
      WindowInsetsCompat.Builder(insets)
        .setInsets(safeArea, Insets.NONE)
        .build()
    }
    ViewCompat.requestApplyInsets(content)
  }
}
