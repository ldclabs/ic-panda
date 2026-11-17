package net.dmsg.android.core

import kotlinx.serialization.json.Json
import net.dmsg.android.BuildConfig
import net.dmsg.core.*

/** Build configuration only: no Intent, deep link or user form can choose an issuer. */
object Deployment {
    suspend fun open(directory: String, staging: String, platform: Platform): MobileEngine {
        if (!BuildConfig.IS_STAGING) return MobileEngine.open(directory, staging, platform)
        if (BuildConfig.DEPLOYMENT.isBlank()) throw MobileException.Failure("STAGING_CONFIGURATION_REQUIRED")
        val config = Json.parseToJsonElement(BuildConfig.DEPLOYMENT).objectValue()
        return MobileEngine.openStaging(directory, staging, platform, StagingDeployment(
            relay = config.required("relay"), handle = config.required("handle"), auth = config.text("auth"),
            userHomes = config.strings("user_homes"), walletHub = config.text("wallet_hub"), connectUrl = config.required("connect_url")))
    }
}
