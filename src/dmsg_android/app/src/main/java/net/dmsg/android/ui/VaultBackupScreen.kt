package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import android.icu.text.BreakIterator
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.serialization.json.*
import net.dmsg.android.R
import net.dmsg.android.core.*
import net.dmsg.android.platform.str
import java.text.DateFormat
import java.text.Normalizer
import java.util.Date
import java.util.Locale

@Composable internal fun BackupApprovalContent(detail: JsonObject) {
    LabelValue(str(R.string.text_items_802a927), detail.requiredUnsigned("items").toString())
    LabelValue(str(R.string.text_attachments_be5733e), detail.requiredUnsigned("files").toString())
    LabelValue(str(R.string.vault_backup_omitted), detail.requiredUnsigned("omitted").toString())
    if (detail.requiredFlag("wallet")) Notice(str(R.string.vault_backup_wallet))
}

private fun validBackupPassword(value: String, repeat: String): Boolean {
    val normalized = Normalizer.normalize(value, Normalizer.Form.NFC)
    if (normalized.toByteArray().size > 256 || normalized != Normalizer.normalize(repeat,Normalizer.Form.NFC)) return false
    val chars=BreakIterator.getCharacterInstance(Locale.ROOT)
    chars.setText(normalized)
    return (1..6).all { chars.next()!=BreakIterator.DONE }
}

@Composable private fun BackupCheck(label:String,checked:Boolean,enabled:Boolean,change:(Boolean)->Unit) {
    Row(Modifier.fillMaxWidth().toggleable(checked,enabled=enabled,role=Role.Checkbox,onValueChange=change),
        verticalAlignment=androidx.compose.ui.Alignment.CenterVertically) {
        Checkbox(checked,onCheckedChange=null,enabled=enabled)
        Text(label,Modifier.padding(8.dp))
    }
}

@Composable private fun BackupPassword(value: String, label: Int, enabled: Boolean, change: (String) -> Unit) {
    var visible by remember(label) { mutableStateOf(false) }
    OutlinedTextField(
        value, change, Modifier.fillMaxWidth(),
        label = { Text(str(label)) }, singleLine = true, enabled = enabled,
        visualTransformation = if (visible) VisualTransformation.None else PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
        trailingIcon = {
            TextButton(onClick = { visible = !visible }, enabled = enabled) {
                Text(str(if (visible) R.string.text_hide_d2e660d else R.string.text_reveal_5917b8a))
            }
        }
    )
}

@Composable fun VaultBackupScreen(client: ClientController) {
    val controller=client.vaultBackup
    val form by controller.form.collectAsStateWithLifecycle()
    val app by client.state.collectAsStateWithLifecycle()
    val working=app.working("vault:backup")
    val pick=rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument(),controller::picked)
    val save=rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/vnd.dmsg.vault-backup+cbor"),controller::destination)
    var password by remember(form.id,form.step) { mutableStateOf("") }
    var confirmation by remember(form.id,form.step) { mutableStateOf("") }
    var include by remember(form.id) { mutableStateOf(true) }
    var folder by remember(form.id) { mutableStateOf("root") }
    var chooseFolder by remember { mutableStateOf(false) }
    val inventory=form.view["inventory"] as? JsonObject
    val rows=(inventory?.get("items") as? JsonArray)?.map { it.jsonObject }.orEmpty()
    val preview=(form.view["items"] as? JsonArray)?.map { it.jsonObject }.orEmpty()
    var selection by remember(form.id,inventory) { mutableStateOf(rows.map { it.required("id") }.toSet()) }
    var picks by remember(form.id,preview) { mutableStateOf(preview.filter { it.required("status")=="new" }.map { it.requiredUInt("index") }.toSet()) }
    val chosen=rows.filter { it.required("id") in selection }
    val files=chosen.flatMap { it.getValue("files").jsonArray.map { f->f.jsonPrimitive.content } }.distinct()
    val fileInfo=inventory?.get("files") as? JsonObject
    val missing=files.any { (fileInfo?.get(it) as? JsonObject)?.requiredFlag("available") != true }
    val bytes = files.sumOf { id ->
        val meta = (fileInfo?.get(id) as? JsonObject)?.get("meta") as? JsonObject
        meta?.requiredUnsigned("size") ?: 0uL
    }
    var processed by remember(form.id, working) { mutableStateOf(0uL) }
    LaunchedEffect(form.id, working) {
        while (working && form.id.isNotEmpty()) {
            kotlinx.coroutines.delay(1000)
            processed = controller.processedBytes(form.id)
        }
    }
    val included=preview.filter { it.requiredUInt("index") in picks }.flatMap { it.getValue("files").jsonArray.filterNot { f->f==JsonNull } }.distinct().size
    LaunchedEffect(form.step) { if (form.step=="waiting") controller.approved() }
    LaunchedEffect(form.failure) { if(form.failure=="INTEGRITY_FAILED") password="" }
    LazyColumn(contentPadding = pagePadding(), verticalArrangement=Arrangement.spacedBy(10.dp)) {
        item { LabelValue(str(R.string.text_account_4ab56e9), app.session?.account.orEmpty()) }
        if(working) item {
            LinearProgressIndicator(Modifier.fillMaxWidth())
            if (processed > 0uL) Text(fileSize(processed))
        }
        if(form.failure!=null) item { Notice(str(if(form.failure=="uncommitted") R.string.vault_backup_uncommitted else R.string.vault_backup_unlockerror)) }
        when(form.step) {
            "home" -> item {
                DetailCard {
                    Text(str(R.string.vault_backup_scope), style = MaterialTheme.typography.bodyMedium)
                    Button(onClick = { controller.begin(false) }, enabled = !working, modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.vault_backup_export))
                    }
                    OutlinedButton(onClick = { controller.begin(true) }, enabled = !working, modifier = Modifier.fillMaxWidth()) {
                        Text(str(R.string.vault_backup_import))
                    }
                    if (controller.previous() != null) TextButton(onClick = controller::reconcile, enabled = !working) { Text(str(R.string.text_review_9a1a906)) }
                }
            }
            "loading" -> item { TextButton(controller::inventory,enabled=!working){Text(str(R.string.text_retry_563171c))} }
            "export" -> {
                item {
                    Text(str(R.string.vault_backup_scope))
                    Text(DateFormat.getDateTimeInstance().format(Date(form.view.requiredUnsigned("at").toLong())))
                    Text(str(R.string.text_trash_8af1079)+" "+inventory?.get("trash")+" · "+str(R.string.text_managed_by_dmsg_35b3681)+" "+inventory?.get("managed"))
                    Text(str(R.string.text_version_conflicts_0df1b3c)+" "+inventory?.get("conflicts"))
                    Text(str(R.string.text_pending_upload_3dce896) + chosen.count { it.text("save_state") == "pending" || (it.text("save_state") == null && !it.requiredFlag("synced")) })
                    Row {
                        TextButton({selection=rows.map { it.required("id") }.toSet()},enabled=!working){Text(str(R.string.text_all_eae0ae1))}
                        TextButton({selection=emptySet()},enabled=!working){Text(str(R.string.text_clear_selection_66ecd44))}
                    }
                }
                items(rows,key={it.required("id")}) { row ->
                    val id=row.required("id")
                    BackupCheck(row.required("title")+" · "+vaultKindName(row.required("kind")),id in selection,!working) {selection=if(it)selection+id else selection-id}
                }
                item {
                    if (files.isNotEmpty()) BackupCheck(str(R.string.vault_backup_files)+" ("+files.size+" · "+fileSize(bytes)+")",include,!working){include=it}
                    if(missing && include) Notice(str(R.string.vault_backup_unavailable))
                    if(chosen.any { it.required("kind")=="wallet" }) Notice(str(R.string.vault_backup_wallet))
                    BackupPassword(password,R.string.vault_backup_password,!working){password=it}
                    Text(str(R.string.vault_backup_hint),style=MaterialTheme.typography.bodySmall)
                    TextButton({controller.generate {password=it}},enabled=!working){Text(str(R.string.text_generate_password_4fe4cd0))}
                    BackupPassword(confirmation,R.string.vault_backup_repeat,!working){confirmation=it}
                    if (confirmation.isNotEmpty() && !Normalizer.normalize(password, Normalizer.Form.NFC)
                            .equals(Normalizer.normalize(confirmation, Normalizer.Form.NFC))) {
                        Text(str(R.string.vault_backup_mismatch), color = MaterialTheme.colorScheme.error)
                    }
                    Button({controller.request(selection.toList(),include,password,confirmation)},
                        enabled=!working && selection.isNotEmpty() && (!include||!missing) && validBackupPassword(password,confirmation)){
                        Text(str(R.string.vault_backup_export)+" ("+selection.size+")")
                    }
                }
            }
            "waiting" -> item {
                TextButton({form.approval?.let(client.approvals::open)},enabled=!working){Text(str(R.string.text_review_approval_db00f36))}
                Button(controller::approved,enabled=!working){Text(str(R.string.text_continue_fd19155))}
            }
            "ready" -> item {
                Text(str(R.string.text_items_802a927)+" "+form.view["items"]+" · "+str(R.string.text_attachments_be5733e)+" "+form.view["files"])
                Button({
                    controller.prepareSave { save.launch("dmsg-vault-" + java.time.LocalDate.now() + ".dmsgvault") }
                }, enabled = !working) { Text(str(R.string.text_save_ec8e6d5)) }
            }
            "pick" -> item {
                Button({if(controller.external())pick.launch(arrayOf("*/*"))},enabled=!working){Text(str(R.string.text_choose_a_file_2916860))}
                form.input?.let { Text(it.substringAfterLast('/')) }
                BackupPassword(password,R.string.vault_backup_password,!working){password=it}
                Button({controller.inspect(password)},enabled=!working && form.input!=null && password.isNotEmpty()){Text(str(R.string.text_review_9a1a906))}
            }
            "preview" -> {
                item {
                    Text(str(R.string.text_source_fcdba0f)+" · "+form.view.required("source"))
                    Text(str(R.string.text_destination_63c1289)+" · "+form.view.required("account"))
                    Text(DateFormat.getDateTimeInstance().format(Date(form.view.requiredUnsigned("at").toLong())))
                    Text(str(R.string.vault_backup_omitted)+" "+form.view["omitted"])
                    Text(str(R.string.vault_backup_new)+" "+preview.count { it.required("status")=="new" }+" · "+str(R.string.vault_backup_same)+" "+preview.count { it.required("status")=="same" }+" · "+str(R.string.text_review_9a1a906)+" "+preview.count { it.required("status") in setOf("changed","deleted") })
                    Row {
                        TextButton({picks=preview.filter { it.required("status")=="new" }.map { it.requiredUInt("index") }.toSet()},enabled=!working){Text(str(R.string.vault_backup_new))}
                        TextButton({picks=emptySet()},enabled=!working){Text(str(R.string.text_clear_selection_66ecd44))}
                    }
                }
                items(preview,key={it.requiredUInt("index").toInt()}) { row ->
                    val index=row.requiredUInt("index")
                    val status=row.required("status")
                    val reason=when(status){"same"->R.string.vault_backup_same;"changed"->R.string.vault_backup_changed;"deleted"->R.string.vault_backup_deleted;else->R.string.vault_backup_new}
                    val refs = row.getValue("files").jsonArray
                    val omitted = refs.count { it == JsonNull }
                    val details = vaultKindName(row.required("kind")) + " · " + str(R.string.text_attachments_be5733e) + " " + (refs.size - omitted) +
                        if (omitted > 0) " · " + str(R.string.vault_backup_omitted) + " " + omitted else ""
                    BackupCheck(row.required("title")+" · "+details+" · "+str(reason),index in picks,!working && status!="same"){picks=if(it)picks+index else picks-index}
                }
                item {
                    if(included>0) Box {
                        OutlinedButton({chooseFolder=true},enabled=!working){Text(str(R.string.text_folder_b0d1441)+" · "+(form.folders.find { it.first==folder }?.second?:str(R.string.text_files_c79ddc2)))}
                        DropdownMenu(chooseFolder,{chooseFolder=false}) {
                            (listOf("root" to str(R.string.text_files_c79ddc2))+form.folders.filterNot { it.first=="root" }).forEach { f ->
                                DropdownMenuItem(text={Text(f.second)},onClick={folder=f.first;chooseFolder=false})
                            }
                        }
                    }
                    if(picks.isEmpty()) TextButton(controller::clear,enabled=!working){Text(str(R.string.text_done_9ffd599))}
                    else Button({controller.commit(picks.sorted(),folder)},enabled=!working){Text(str(R.string.vault_backup_import)+" ("+picks.size+" · "+str(R.string.text_attachments_be5733e)+" "+included+")")}
                }
            }
            "done" -> item {
                Text(str(R.string.vault_backup_saved))
                Text(str(R.string.text_items_802a927)+" "+form.view["items"]+" · "+str(R.string.text_attachments_be5733e)+" "+form.view["files"])
                Text(str(R.string.vault_backup_omitted)+" "+form.view["omitted"])
                if("skipped" in form.view) Text(str(R.string.vault_backup_sync))
                TextButton(controller::clear){Text(str(R.string.text_done_9ffd599))}
            }
        }
    }
}
