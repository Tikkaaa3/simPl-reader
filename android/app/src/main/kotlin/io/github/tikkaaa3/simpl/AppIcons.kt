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
    val Close = icon("Close", "M6.4,5L12,10.6L17.6,5L19,6.4L13.4,12L19,17.6L17.6,19L12,13.4L6.4,19L5,17.6L10.6,12L5,6.4Z")
    val More = icon("More", "M12,4A2,2 0 1,0 12,8A2,2 0 1,0 12,4M12,10A2,2 0 1,0 12,14A2,2 0 1,0 12,10M12,16A2,2 0 1,0 12,20A2,2 0 1,0 12,16")
    val Heart = icon("Favorite", "M12,21L3.5,12.5C-2,7 5,0 12,6C19,0 26,7 20.5,12.5Z")
    val Plus = icon("Add", "M11,4H13V11H20V13H13V20H11V13H4V11H11Z")
    val Tune = icon("Reading options", "M4,5H20V7H4ZM4,11H20V13H4ZM4,17H20V19H4ZM7,3H9V9H7ZM15,9H17V15H15ZM7,15H9V21H7Z")
    // Material "settings" gear (Apache-2.0), matching the desktop toolbar.
    val Settings = icon("Settings", "M19.14,12.94c0.04,-0.3 0.06,-0.61 0.06,-0.94c0,-0.32 -0.02,-0.64 -0.07,-0.94l2.03,-1.58c0.18,-0.14 0.23,-0.41 0.12,-0.61l-1.92,-3.32c-0.12,-0.22 -0.37,-0.29 -0.59,-0.22l-2.39,0.96c-0.5,-0.38 -1.03,-0.7 -1.62,-0.94L14.4,2.81c-0.04,-0.24 -0.24,-0.41 -0.48,-0.41h-3.84c-0.24,0 -0.43,0.17 -0.47,0.41L9.25,5.35C8.66,5.59 8.12,5.92 7.63,6.29L5.24,5.33c-0.22,-0.08 -0.47,0 -0.59,0.22L2.74,8.87C2.62,9.08 2.66,9.34 2.86,9.48l2.03,1.58C4.84,11.36 4.8,11.69 4.8,12s0.02,0.64 0.07,0.94l-2.03,1.58c-0.18,0.14 -0.23,0.41 -0.12,0.61l1.92,3.32c0.12,0.22 0.37,0.29 0.59,0.22l2.39,-0.96c0.5,0.38 1.03,0.7 1.62,0.94l0.36,2.54c0.05,0.24 0.24,0.41 0.48,0.41h3.84c0.24,0 0.44,-0.17 0.47,-0.41l0.36,-2.54c0.59,-0.24 1.13,-0.56 1.62,-0.94l2.39,0.96c0.22,0.08 0.47,0 0.59,-0.22l1.92,-3.32c0.12,-0.22 0.07,-0.47 -0.12,-0.61L19.14,12.94zM12,15.6c-1.98,0 -3.6,-1.62 -3.6,-3.6s1.62,-3.6 3.6,-3.6s3.6,1.62 3.6,3.6S13.98,15.6 12,15.6z")
    val HeartOutline = icon("Favorite outline", "M12,21L3.5,12.5C-2,7 5,0 12,6C19,0 26,7 20.5,12.5ZM12,18.2L19.1,11.1C22.5,7.6 17.6,3.2 13.3,7.4L12,8.6L10.7,7.4C6.4,3.2 1.5,7.6 4.9,11.1Z")
    val ChevronRight = icon("Open", "M9.4,6L15.4,12L9.4,18L8,16.6L12.6,12L8,7.4Z")
    val Copy = icon("Copy", "M8,7H19V22H8ZM10,9V20H17V9ZM5,2H15V4H7V18H5Z")
    val Share = icon("Share", "M18,16C17.2,16 16.6,16.3 16,16.8L8.9,12.7C9,12.5 9,12.2 9,12C9,11.8 9,11.5 8.9,11.3L16,7.2C16.5,7.7 17.2,8 18,8A3,3 0 1,0 15,5C15,5.2 15,5.5 15.1,5.7L8,9.8C7.5,9.3 6.8,9 6,9A3,3 0 1,0 6,15C6.8,15 7.5,14.7 8,14.2L15.1,18.3C15,18.5 15,18.7 15,19A3,3 0 1,0 18,16Z")
    val Dictionary = icon("Dictionary", "M6,2H19V18H7A1,1 0 0,0 7,20H19V22H7A3,3 0 0,1 4,19V4A2,2 0 0,1 6,2ZM6,4V16.2C6.3,16.1 6.6,16 7,16H17V4Z")
    val Speak = icon("Read aloud", "M3,9H7L12,4V20L7,15H3ZM14,7.2C15.8,8 17,9.9 17,12C17,14.1 15.8,16 14,16.8V14.5C14.6,13.9 15,13 15,12C15,11 14.6,10.1 14,9.5ZM14,3.2C17.9,4.1 21,7.7 21,12C21,16.3 17.9,19.9 14,20.8V18.7C16.8,17.9 19,15.2 19,12C19,8.8 16.8,6.1 14,5.3Z")
    val Note = icon("Add note", "M4,4H20V16H8L4,20ZM6,6V15.2L7.2,14H18V6Z")
    val Search = icon("Search", "M10,3A7,7 0 1,0 10,17A7,7 0 1,0 10,3M10,5A5,5 0 1,1 10,15A5,5 0 1,1 10,5M15,14L21,20L20,21L14,15Z")
    val Previous = icon("Previous page", "M15.4,5L8.4,12L15.4,19L14,20.4L5.6,12L14,3.6Z")
    val Next = icon("Next page", "M8.6,5L15.6,12L8.6,19L10,20.4L18.4,12L10,3.6Z")
    val Contents = icon("Contents", "M4,4H6V6H4ZM9,4H20V6H9ZM4,11H6V13H4ZM9,11H20V13H9ZM4,18H6V20H4ZM9,18H20V20H9Z")
    val Bookmark = icon("Bookmark", "M6,3H18V22L12,18L6,22Z")
    val BookmarkOutline = icon("Bookmark outline", "M6,3H18V22L12,18L6,22ZM8,5V18.3L12,15.7L16,18.3V5Z")
    val Notes = icon("Annotations", "M4,3H20V21H4ZM6,5V19H18V5ZM8,8H16V10H8ZM8,12H16V14H8ZM8,16H13V17H8Z")
    val Switch = icon("Switch book", "M4,4H10V20H4ZM6,6V18H8V6ZM12,4H14V20H12ZM16,4H18L21,19L19,20Z")
    val Check = icon("Go", "M9,16.2L4.8,12L3.4,13.4L9,19L21,7L19.6,5.6Z")
    val Hide = icon("Hide controls", "M4,4H9V6H6V9H4ZM15,4H20V9H18V6H15ZM4,15H6V18H9V20H4ZM18,15H20V20H15V18H18Z")
    val Pause = icon("Pause", "M6,4H10V20H6ZM14,4H18V20H14Z")
    val Play = icon("Resume", "M7,4L21,12L7,20Z")
    val Stop = icon("Stop", "M5,5H19V19H5Z")
}
