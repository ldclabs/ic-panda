package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Per-resource state; its request number prevents older reads replacing a newer edit. */
class ReadStore<T>(private val empty: (T) -> Boolean = { false }) {
    private var revision = 0L
    private val mutable = MutableStateFlow<ReadState<T>>(ReadState.Idle)
    val state = mutable.asStateFlow()
    fun value(): T? = mutable.value.valueOrNull()
    fun begin(keep: Boolean = true): Long {
        val next = ++revision
        mutable.value = ReadState.Loading(if (keep) value() else null)
        return next
    }
    fun accept(request: Long, value: T) {
        if (request == revision) mutable.value = if (empty(value)) ReadState.Empty(value) else ReadState.Ready(value)
    }
    fun fail(request: Long, code: String) {
        if (request != revision) return
        val previous = value()
        mutable.value = if (previous != null) ReadState.Stale(previous, code) else ReadState.Failed(code)
    }
    fun reset() { revision += 1; mutable.value = ReadState.Idle }
}
