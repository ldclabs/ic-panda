package net.dmsg.android.core

import android.content.Context

object NativeBootstrap {
    init { System.loadLibrary("dmsg_mobile") }

    @JvmStatic external fun initialize(context: Context): Boolean
}
