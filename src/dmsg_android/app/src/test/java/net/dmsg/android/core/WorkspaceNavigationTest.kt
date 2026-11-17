package net.dmsg.android.core

import org.junit.Assert.*
import org.junit.Test

class WorkspaceNavigationTest {
    @Test fun workspaceStacksAndAccountHistoryRemainIndependent() {
        val nav = WorkspaceNavigation(); nav.activate("a")
        nav.select("secret-a"); nav.open("messages"); nav.select("channel-a")
        nav.open("vault"); assertEquals("secret-a", nav.state.value.current.id)
        nav.activate("b"); assertNull(nav.state.value.current.id)
        nav.select("secret-b"); nav.activate("a")
        assertEquals("secret-a", nav.state.value.current.id)
        nav.back(); assertNull(nav.state.value.current.id)
        nav.open("messages"); assertEquals("channel-a", nav.state.value.current.id)
    }
    @Test fun processStateContainsOnlyRoutesAndRestoresNestedFolderBackPath() {
        val nav = WorkspaceNavigation(); nav.activate("a"); nav.open("files")
        nav.folder("folder-1"); nav.folder("folder-2"); nav.select("file")
        val restored = WorkspaceNavigation(); restored.restore(nav.save()); restored.activate("a")
        assertEquals("file", restored.state.value.current.id)
        restored.back(); assertEquals("folder-2", restored.state.value.current.folder)
        restored.back(); assertEquals("folder-1", restored.state.value.current.folder)
        restored.back(); assertEquals("root", restored.state.value.current.folder)
        restored.folder("folder-1"); restored.folder("folder-2"); restored.folder("folder-1")
        restored.back(); assertEquals("root", restored.state.value.current.folder)
        assertFalse(restored.state.value.canGoBack)
    }

    @Test fun listPositionsSurviveRecreationAndLateDisposalKeepsOriginalAccount() {
        val nav = WorkspaceNavigation(); nav.activate("a")
        val owner = nav.scrollOwner()
        nav.rememberScroll(owner, "vault:list", WorkspaceNavigation.ScrollPosition(37, 18))
        nav.open("messages"); nav.rememberScroll(owner, "messages:list", WorkspaceNavigation.ScrollPosition(4, 9))
        nav.activate("b")
        nav.rememberScroll(owner, "vault:list", WorkspaceNavigation.ScrollPosition(38, 5))
        assertEquals(WorkspaceNavigation.ScrollPosition(), nav.scrollPosition("vault:list"))
        val restored = WorkspaceNavigation(); restored.restore(nav.save()); restored.activate("a")
        assertEquals(WorkspaceNavigation.ScrollPosition(38, 5), restored.scrollPosition("vault:list"))
        assertEquals(WorkspaceNavigation.ScrollPosition(4, 9), restored.scrollPosition("messages:list"))
        restored.forget("a"); restored.activate("a")
        assertEquals(WorkspaceNavigation.ScrollPosition(), restored.scrollPosition("vault:list"))
    }
}
