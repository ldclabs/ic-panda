package net.dmsg.android.core

import kotlinx.coroutines.Job

/** Resource ownership makes a late completion incapable of removing its replacement. */
class OperationOwners {
    private val owners = mutableMapOf<String, Job>()
    @Synchronized fun running(key: String): Boolean = owners[key]?.isActive == true
    @Synchronized fun keys(): Set<String> = owners.keys.toSet()
    @Synchronized fun claim(key: String, job: Job, replace: Boolean): Boolean {
        val old = owners[key]
        if (old != null && !old.isCompleted && !replace) return false
        owners[key] = job
        old?.cancel()
        return true
    }
    @Synchronized fun owns(key: String, job: Job): Boolean = owners[key] === job
    @Synchronized fun release(key: String, job: Job) { if (owners[key] === job) owners.remove(key) }
    @Synchronized fun cancel(key: String): Job? = owners.remove(key)?.also { it.cancel() }
    @Synchronized fun cancelAll() { owners.values.toList().forEach { it.cancel() }; owners.clear() }
}
