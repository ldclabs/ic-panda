package net.dmsg.android.core

import net.dmsg.core.MobileEngine
import net.dmsg.core.SessionSnapshot

data class SessionStamp(val epoch: Long, val generation: ULong, val account: String?)

/** UI invalidation and native generations are separate barriers. */
class SessionFence {
    @Volatile var epoch: Long = 0
        private set
    private var generation: ULong? = null
    private var sequence: ULong? = null
    private var account: String? = null
    @Synchronized fun invalidate() { epoch += 1 }
    @Synchronized fun adopt(nextGeneration: ULong, nextSequence: ULong, nextAccount: String?): Boolean {
        val old = generation
        if (old != null && (nextGeneration < old || nextGeneration == old && sequence?.let { nextSequence <= it } == true)) return false
        if (old != null && (nextAccount != account || nextGeneration != old)) epoch += 1
        generation = nextGeneration; sequence = nextSequence; account = nextAccount
        return true
    }
    @Synchronized fun stamp(): SessionStamp? = generation?.let { SessionStamp(epoch, it, account) }
    @Synchronized fun current(stamp: SessionStamp): Boolean = stamp.epoch == epoch && stamp.generation == generation && stamp.account == account
}

/** The Application owns one root; each operation owns and releases a scoped Arc. */
class NativeSession {
    val fence = SessionFence()
    var root: MobileEngine? = null
        private set
    var snapshot: SessionSnapshot? = null
        private set
    fun install(engine: MobileEngine) { check(root == null); root = engine }
    fun adopt(next: SessionSnapshot): Boolean {
        val owner = root ?: return false
        if (owner.controlGeneration() != next.generation) return false
        if (!fence.adopt(next.generation, next.sequence, next.account)) return false
        snapshot = next
        return true
    }
    fun handle(): MobileEngine {
        val owner = root ?: error("LOCKED")
        val state = snapshot ?: error("LOCKED")
        return owner.scoped(state.generation)
    }
}
