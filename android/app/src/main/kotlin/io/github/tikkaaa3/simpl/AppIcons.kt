package io.github.tikkaaa3.simpl

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.unit.dp

/** Small, local vector set keeps the library independent of an icon font. */
object AppIcons {
    private fun icon(name: String, path: String) = ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f)
        .addPath(addPathNodes(path), fill = SolidColor(Color.Black)).build()
    val Back = icon("Back", "M20,11H7.8L13.4,5.4L12,4L4,12L12,20L13.4,18.6L7.8,13H20Z")
    val More = icon("More", "M12,4A2,2 0 1,0 12,8A2,2 0 1,0 12,4M12,10A2,2 0 1,0 12,14A2,2 0 1,0 12,10M12,16A2,2 0 1,0 12,20A2,2 0 1,0 12,16")
    val Heart = icon("Favorite", "M12,21L3.5,12.5C-2,7 5,0 12,6C19,0 26,7 20.5,12.5Z")
    val Plus = icon("Add", "M11,4H13V11H20V13H13V20H11V13H4V11H11Z")
    val Settings = icon("Settings", "M4,5H20V7H4ZM4,11H20V13H4ZM4,17H20V19H4ZM7,3H9V9H7ZM15,9H17V15H15ZM7,15H9V21H7Z")
    val Search = icon("Search", "M10,3A7,7 0 1,0 10,17A7,7 0 1,0 10,3M10,5A5,5 0 1,1 10,15A5,5 0 1,1 10,5M15,14L21,20L20,21L14,15Z")
}
