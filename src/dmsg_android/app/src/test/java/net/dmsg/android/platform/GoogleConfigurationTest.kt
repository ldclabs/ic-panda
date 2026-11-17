package net.dmsg.android.platform

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class GoogleConfigurationTest {
    private fun config() = JSONObject().put("package_name", "net.dmsg.android").put("environment", "production").put("firebase_app_id", "1:123456789:android:abcdef0123").put("api_key", "test-public-api-key-not-a-secret").put("project_id", "dmsg-test-project").put("cloud_project_number", "123456789")
    @Test fun fixedPackageEnvironmentAndProjectMustAgree() {
        assertNotNull(GoogleConfiguration.parse(config().toString(), "net.dmsg.android", "production"))
        assertNull(GoogleConfiguration.parse(config().toString(), "net.dmsg.android.dev", "production"))
        assertNull(GoogleConfiguration.parse(config().toString(), "net.dmsg.android", "staging"))
        assertNull(GoogleConfiguration.parse(config().put("cloud_project_number", "987654321").toString(), "net.dmsg.android", "production"))
        assertNull(GoogleConfiguration.parse(config().put("private_key", "never-load-service-credentials").toString(), "net.dmsg.android", "production"))
        assertNull(GoogleConfiguration.parse("", "net.dmsg.android", "production"))
    }
}
