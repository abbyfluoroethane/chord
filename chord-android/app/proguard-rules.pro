# R8 rules for the Chord release build.

# JNA: native code looks up these classes and fields by name. The aar has consumer rules
# for most of it. These lines make sure.
-dontwarn java.awt.*
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.Structure { <fields>; }

# UniFFI bindings: Structure subclasses (field order by reflection), callback interfaces
# that Rust calls, and the library loader. Keep the whole generated package.
-keep class uniffi.** { *; }

# JNI methods that the Rust core registers by name.
-keep class space.foid.chord.NativeInit { *; }

# rustls-platform-verifier: Rust calls this Kotlin class through JNI.
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
