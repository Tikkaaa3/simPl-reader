# JNA binds the generated UniFFI interfaces by reflection.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-dontwarn java.awt.**

# Generated UniFFI bindings for the Rust core.
-keep class io.github.tikkaaa3.simpl.core.** { *; }
