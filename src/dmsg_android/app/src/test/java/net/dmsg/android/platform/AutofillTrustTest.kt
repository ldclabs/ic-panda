package net.dmsg.android.platform

import org.junit.Assert.*
import org.junit.Test

class AutofillTrustTest {
    @Test fun credentialSharingRequiresTheExactPackageAndEveryCurrentSigner() {
        val a = "AA".repeat(32); val b = "BB".repeat(32)
        val statement = """[{"relation":["delegate_permission/common.get_login_creds"],"target":{"namespace":"android_app","package_name":"sample.app","sha256_cert_fingerprints":["$a"]}}]"""
        assertTrue(AutofillTrust.statement(statement, "sample.app", setOf(a)))
        assertFalse(AutofillTrust.statement(statement, "evil.app", setOf(a)))
        assertFalse(AutofillTrust.statement(statement, "sample.app", setOf(b)))
        assertFalse(AutofillTrust.statement(statement, "sample.app", setOf(a, b)))
        assertFalse(AutofillTrust.statement(statement.replace("get_login_creds", "handle_all_urls"), "sample.app", setOf(a)))
        assertFalse(AutofillTrust.statement("{}", "sample.app", setOf(a)))
    }
    @Test fun requestHostsNeverBroadenToParentSitesOrAcceptHttpUserInfoAndLocalAddresses() {
        assertEquals("login.example.com", AutofillTrust.host("https://login.example.com/path"))
        assertEquals("login.example.com", AutofillTrust.host("login.example.com"))
        assertNotEquals(AutofillTrust.host("example.com"), AutofillTrust.host("evil.example.com"))
        listOf("http://example.com", "https://example.com@evil.com", "https://127.0.0.1", "https://localhost", "https://[::1]", "https://example.com:8080", "https://example.com.").forEach { assertNull(it, AutofillTrust.host(it)) }
    }
}
