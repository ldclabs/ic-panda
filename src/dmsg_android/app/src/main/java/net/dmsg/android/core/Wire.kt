package net.dmsg.android.core

import kotlinx.serialization.json.*
import net.dmsg.core.ObjectSnapshot

/** JSON integers are kept as lexical numbers. No money, version or u64 uses Double. */
fun ObjectSnapshot.decode(): JsonElement = Json.parseToJsonElement(json)
fun JsonElement.objectValue(): JsonObject = this as? JsonObject ?: error("INTEGRITY_FAILED")
fun JsonObject.text(key: String): String? = when (val value = this[key]) {
    null, JsonNull -> null
    is JsonPrimitive -> if (value.isString) value.content else error("INTEGRITY_FAILED")
    else -> error("INTEGRITY_FAILED")
}
fun JsonObject.required(key: String): String = text(key) ?: error("INTEGRITY_FAILED")
fun JsonObject.unsigned(key: String): ULong? = when (val value = this[key]) {
    null, JsonNull -> null
    is JsonPrimitive -> value.content.takeIf { !value.isString && it.isNotEmpty() && it.all { char -> char in '0'..'9' } }
        ?.toULongOrNull() ?: error("INTEGRITY_FAILED")
    else -> error("INTEGRITY_FAILED")
}
fun JsonObject.requiredUnsigned(key: String): ULong = unsigned(key) ?: error("INTEGRITY_FAILED")
fun JsonObject.requiredUInt(key: String): UInt = requiredUnsigned(key).also { check(it <= UInt.MAX_VALUE.toULong()) { "INTEGRITY_FAILED" } }.toUInt()
fun JsonObject.requiredFlag(key: String): Boolean {
    check(containsKey(key) && this[key] != JsonNull) { "INTEGRITY_FAILED" }
    return flag(key)
}
fun JsonObject.flag(key: String, default: Boolean = false): Boolean =
    when (val value = this[key]) {
        null, JsonNull -> default
        is JsonPrimitive -> if (!value.isString) value.booleanOrNull ?: error("INTEGRITY_FAILED") else error("INTEGRITY_FAILED")
        else -> error("INTEGRITY_FAILED")
    }
fun JsonElement.rows(key: String? = null): List<JsonObject> = when (this) {
    is JsonArray -> map { it.objectValue() }
    is JsonObject -> if (key == null) listOf(this) else (this[key] as? JsonArray)?.map { it.objectValue() } ?: error("INTEGRITY_FAILED")
    else -> error("INTEGRITY_FAILED")
}

fun JsonElement.arrayRows(): List<JsonObject> = (this as? JsonArray)?.map { it.objectValue() } ?: error("INTEGRITY_FAILED")
fun JsonObject.strings(key: String): List<String> = (this[key] as? JsonArray)?.map {
    val value = it as? JsonPrimitive ?: error("INTEGRITY_FAILED")
    check(value.isString) { "INTEGRITY_FAILED" }; value.content
} ?: error("INTEGRITY_FAILED")

sealed interface ReadState<out T> {
    data object Idle : ReadState<Nothing>
    data class Loading<T>(val previous: T? = null) : ReadState<T>
    data class Empty<T>(val value: T) : ReadState<T>
    data class Ready<T>(val value: T) : ReadState<T>
    data class Stale<T>(val value: T, val reason: String) : ReadState<T>
    data class Failed<T>(val code: String, val previous: T? = null) : ReadState<T>
}

fun <T> ReadState<T>.valueOrNull(): T? = when (this) {
    is ReadState.Ready -> value
    is ReadState.Empty -> value
    is ReadState.Stale -> value
    is ReadState.Loading -> previous
    is ReadState.Failed -> previous
    ReadState.Idle -> null
}
