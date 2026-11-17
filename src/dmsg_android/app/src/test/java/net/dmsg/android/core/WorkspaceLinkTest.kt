package net.dmsg.android.core

import org.junit.Assert.*
import org.junit.Test

class WorkspaceLinkTest {
    @Test fun systemEntriesOnlySelectKnownDestinationsAndContainNoAuthority() {
        for (destination in WorkspaceLink.pages.keys) {
            val entry = WorkspaceLink(destination)
            assertEquals(entry, WorkspaceLink.parse(entry.url()))
        }
        assertEquals("incoming-shares", WorkspaceLink("imports").page)
        for (value in listOf("https://open/vault", "dmsg://user@open/vault", "dmsg://open:12/vault", "dmsg://open/%76ault", "dmsg://open/vault/", "dmsg://open/../vault", "dmsg://open/approvals?approve=true", "dmsg://open/files#secret", "dmsg://open/imports?account=foreign", "dmsg://open/unknown")) assertNull(value, WorkspaceLink.parse(value))
    }
}
