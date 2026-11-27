package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class NearbyShareModelsTest {
    @Test fun groupedNearbySasSplitsSixDigits() {
        assertEquals("482\u00a0913", groupedNearbySas("482913"))
        assertEquals("1234", groupedNearbySas("1234"))
    }

    @Test fun parsesReceiveAndSendNearbyStatus() {
        val payload = """
        {
          "receive": {
            "id": 7,
            "state": "waiting",
            "alias": "Alice Phone",
            "discoverable": false,
            "advert": "dmsg-0123456789abcdef",
            "pk_hint": "ab12cd34",
            "port": 41234,
            "addrs": ["192.168.1.20:41234"],
            "offer": "dmsg-nearby/1:test",
            "expires_at": 1730000000000,
            "relay_ready": true,
            "transport": null,
            "verification_codes": ["084391"],
            "pending_peers": [
              {
                "peer_id": "p1",
                "sas": "019283",
                "qr_bound": true,
                "transport": "lan",
                "kind": "files",
                "files": [{"index": 0, "name": "report.pdf", "mime": "application/pdf", "size": 2048}],
                "package_size": null,
                "alias": "Bob Laptop"
              }
            ],
            "active_peer": null,
            "progress": {"total_items": 1, "completed_items": 0, "total_bytes": 2048, "bytes": 0, "current_name": null},
            "outcome": null,
            "error": null
          },
          "send": {
            "id": 9,
            "state": "awaiting_approval",
            "kind": "secret_package",
            "transport": "relay",
            "sas": "654321",
            "transcript": "deadbeef",
            "qr_bound": true,
            "peer_label": "Carol iPhone",
            "share_job_id": "job-1",
            "approval_id": "apr-1",
            "progress": {"total_items": 1, "completed_items": 0, "total_bytes": 512, "bytes": 0, "current_name": null},
            "outcome": null,
            "error": null
          }
        }
        """
        val parsed = parseNearbyStatus(Json.parseToJsonElement(payload))
        val recv = requireNotNull(parsed.receive)
        assertTrue(recv.active)
        assertEquals(41234.toUShort(), recv.port)
        assertTrue(recv.relayReady)
        assertEquals(listOf("084391"), recv.verificationCodes)
        assertEquals(1, recv.pendingPeers.size)
        assertEquals("report.pdf", recv.pendingPeers.single().files.single().name)
        val send = requireNotNull(parsed.send)
        assertTrue(send.active)
        assertEquals("apr-1", send.approvalId)
        assertEquals("654321", send.sas)
    }
}
