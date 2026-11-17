package net.dmsg.android.core

import kotlinx.coroutines.Job
import org.junit.Assert.*
import org.junit.Test

class ReadStoreTest {
    @Test fun loadingFailureIsNotAnEmptyListAndAnOldReplyCannotReplaceANewRead() {
        val store = ReadStore<List<String>> { it.isEmpty() }
        val first = store.begin()
        store.fail(first, "UNAVAILABLE")
        assertTrue(store.state.value is ReadState.Failed)
        val old = store.begin()
        val newer = store.begin()
        store.accept(newer, listOf("new"))
        store.accept(old, listOf("stale"))
        assertEquals(listOf("new"), store.value())
        val failed = store.begin()
        store.fail(failed, "OFFLINE")
        assertTrue(store.state.value is ReadState.Stale)
        store.reset()
        store.accept(failed, listOf("other account"))
        assertNull(store.value())
    }

    @Test fun replacedOperationCannotRemoveItsSuccessorOrJoinAnotherResource() {
        val owners = OperationOwners()
        val old = Job(); val next = Job(); val file = Job()
        assertTrue(owners.claim("vault:list", old, false))
        assertFalse(owners.claim("vault:list", Job(), false))
        assertTrue(owners.claim("file:upload", file, false))
        assertTrue(owners.claim("vault:list", next, true))
        assertTrue(old.isCancelled)
        owners.release("vault:list", old)
        assertTrue(owners.owns("vault:list", next))
        assertTrue(file.isActive)
        owners.cancelAll()
        assertTrue(next.isCancelled); assertTrue(file.isCancelled)
        assertTrue(owners.keys().isEmpty())
    }
}
