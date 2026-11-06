import SwiftUI
@preconcurrency import DMsgCore

struct VaultView: View {
    @Environment(SessionModel.self) private var model
    @State private var editing: VaultEditorTarget?
    @State private var trash = false
    @State private var filtering = false
    @State private var copyingPassword: VaultItem?

    var body: some View {
        @Bindable var model = model
        WorkspaceNavigation(selection: $model.route.vaultItem, sidebarInitiallyVisible: true) { selection in
            VStack(spacing: 0) {
                VaultBrowseBar(filtering: $filtering)
                List(selection: selection) {
                    Group {
                    if model.vaultQueryIsCurrent {
                        ForEach(model.vault, id: \.id) { item in
                            NavigationLink(value: item.id) { VaultRow(item: item) }
                                .compactObjectRow().accessibilityIdentifier("vault.item.\(item.id)")
                                .swipeActions { if item.kind == "login" { copyActions(item) } }
                                .contextMenu { if item.kind == "login" { copyActions(item) } }
                        }
                    }
                    if model.loadingVault || !model.vaultQueryIsCurrent && !model.vaultFailed {
                        ProgressView().frame(maxWidth: .infinity).listRowBackground(Color.clear)
                            .accessibilityIdentifier("vault.loading")
                    }
                    if model.vaultFailed {
                        NoticeBar(text: String(localized: "The operation could not be completed. Please try again."))
                        Button("Retry") { Task { await model.loadVault() } }.accessibilityIdentifier("vault.retry")
                    } else if model.vaultLoaded && model.vaultQueryIsCurrent && model.vault.isEmpty && !model.loadingVault { empty }
                    if model.vaultQueryIsCurrent && model.vaultHasMore {
                        LoadMoreRow(working: model.loadingVault, trigger: model.vault.count) { await model.loadVault(more: true) }
                            .accessibilityIdentifier("vault.more")
                    }
                    }.plainListRows(selectable: selection != nil)
                }.listStyle(.plain).refreshable { await model.synchronize() }
            }.privateScreen().navigationTitle("Vault")
                .searchable(text: $model.search, placement: .navigationBarDrawer(displayMode: .always), prompt: "Search your vault")
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) { ActivityButton() }
                    ToolbarItemGroup(placement: .primaryAction) {
                        Button("Add item", systemImage: "plus") { add() }.accessibilityIdentifier("vault.add").keyboardShortcut("n", modifiers: .command)
                        Menu("Vault tools", systemImage: "ellipsis.circle") {
                            NavigationLink("Import passwords") { VaultImportView() }
                            NavigationLink("Import secret package") { SecretPackageImportView() }
                            NavigationLink("Shared snapshots") { SecretSharesView() }
                            NavigationLink("Local security review") { SecretHealthView() }
                            NavigationLink("Password AutoFill") { SystemExportsView(section: .autoFill) }
                            NavigationLink("Activity on this device") { SecretActivityView() }
                            Button("Trash", systemImage: "trash") { trash = true }
                        }
                    }
                }
                .task(id: model.vaultQueryKey) {
                    try? await Task.sleep(for: .milliseconds(200))
                    if !Task.isCancelled { await model.loadVault() }
                }
        } detail: { id in
            if let id { VaultDetailView(id: id) }
            else { ContentUnavailableView("Select a secret", image: AppGlyph.vault.assetName,
                description: Text("Your hidden fields stay concealed until you choose to reveal them.")) }
        }
        .onChange(of: model.route.vaultItem) { _, value in if value != nil { model.activity() } }
        .sheet(item: $editing) { target in NavigationStack { VaultEditor(target: target) } }
        .sheet(isPresented: $trash) { NavigationStack { VaultTrashView() } }
        .sheet(isPresented: $filtering) { NavigationStack { VaultFiltersView() } }
        .confirmationDialog("Copy this field?", isPresented: Binding(get: { copyingPassword != nil }, set: { if !$0 { copyingPassword = nil } }), titleVisibility: .visible) {
            Button("Copy for 30 seconds") { if let item = copyingPassword { Task { await model.copyVaultField(item, field: "password") } } }
        } message: { Text("Copies stay on this device and expire within 30 seconds.") }
    }

    @ViewBuilder private func copyActions(_ item: VaultItem) -> some View {
        Button("Copy password", systemImage: "key") { copyingPassword = item }.tint(Theme.forest)
        Button("Copy username", systemImage: "person") { Task { await model.copyVaultField(item, field: "username") } }
    }

    @ViewBuilder private var empty: some View {
        if model.contentSync?.initialized != true {
            ContentUnavailableView("Waiting for first sync", systemImage: "arrow.triangle.2.circlepath")
        } else if !model.search.isEmpty {
            ContentUnavailableView.search(text: model.search)
            Button("Clear filters") { clear() }
        } else {
            ContentUnavailableView(model.vaultFilter == "favorites" ? "No favorites" : "No items", image: AppGlyph.vault.assetName)
            Button("Add item", systemImage: "plus") { add() }
            if model.vaultFilter == "all" && model.vaultTag.isEmpty {
                NavigationLink("Import passwords") { VaultImportView() }
            } else { Button("Clear filters") { clear() } }
        }
    }

    private func clear() { model.search = ""; model.vaultTag = ""; model.vaultFilter = "all" }

    private func add() {
        editing = VaultEditorTarget(item: nil, kind: vaultKinds().contains(model.vaultFilter) ? model.vaultFilter : "login", favorite: model.vaultFilter == "favorites")
    }
}

private struct VaultBrowseBar: View {
    @Environment(SessionModel.self) private var model
    @Binding var filtering: Bool
    private var moreSelected: Bool { vaultKinds().contains(model.vaultFilter) && !["login", "note"].contains(model.vaultFilter) }

    var body: some View {
        HStack(spacing: 0) {
            ScrollViewReader { proxy in
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        scope("all", title: "All", symbol: "square.grid.2x2")
                        scope("favorites", title: "Favorites", symbol: "star")
                        scope("login", title: "Passwords", symbol: "key")
                        scope("note", title: "Note", symbol: "note.text")
                        Menu {
                            ForEach(vaultKinds(), id: \.self) { kind in
                                Button { model.vaultFilter = kind } label: {
                                    Label {
                                        if model.vaultCountsAreCurrent && model.contentSync?.initialized == true {
                                            Text(vaultKindName(kind)) + Text(verbatim: " · \(model.vaultCounts.count(kind))")
                                        } else { Text(vaultKindName(kind)) }
                                    } icon: { AppGlyph.vaultItem(kind).image }
                                }
                            }
                        } label: {
                            HStack(spacing: 5) {
                                Text(moreSelected ? vaultKindName(model.vaultFilter) : "More types")
                                Image(systemName: "chevron.down").font(.caption)
                            }.browseTabShape(selected: moreSelected)
                        }.id("types").accessibilityAddTraits(moreSelected ? .isSelected : []).accessibilityIdentifier("vault.types")
                    }.padding(.horizontal, 16)
                }
                .onChange(of: model.vaultFilter) { _, filter in
                    // Reveal an offscreen selection without recentering visible tabs.
                    proxy.scrollTo(["all", "favorites", "login", "note"].contains(filter) ? filter : "types")
                }
            }
            Button { filtering = true } label: {
                Image(systemName: model.vaultTag.isEmpty ? "line.3.horizontal.decrease" : "line.3.horizontal.decrease.circle.fill").frame(width: 44, height: 44)
            }.accessibilityLabel("Filters").accessibilityIdentifier("vault.filters")
        }.font(.subheadline).padding(.bottom, 8).background(Theme.paper)
    }

    private func scope(_ value: String, title: LocalizedStringKey, symbol: String) -> some View {
        BrowseTab(selected: model.vaultFilter == value) { model.vaultFilter = value } label: {
            if ["login", "note"].contains(value) {
                AppGlyph.vaultItem(value).image.resizable().scaledToFit().frame(width: 17, height: 17).accessibilityHidden(true)
            } else { Image(systemName: symbol) }
            Text(title)
        }.id(value).accessibilityIdentifier("vault.scope.\(value)")
    }
}

private struct VaultFiltersView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        @Bindable var model = model
        Form {
            Picker("Type", selection: $model.vaultFilter) {
                Text("All").tag("all"); Text("Favorites").tag("favorites")
                ForEach(vaultKinds(), id: \.self) { Text(vaultKindName($0)).tag($0) }
            }
            Picker("Sort", selection: $model.vaultSort) { Text("Recently updated").tag("updated"); Text("Name").tag("title") }.pickerStyle(.inline)
            Picker("Tags", selection: $model.vaultTag) {
                Text("All").tag("")
                ForEach(model.vaultCounts.tags, id: \.self) { Text($0).tag($0) }
                if !model.vaultTag.isEmpty && !model.vaultCounts.tags.contains(model.vaultTag) { Text(model.vaultTag).tag(model.vaultTag) }
            }.pickerStyle(.inline)
            Button("Clear filters") { model.vaultTag = ""; model.vaultFilter = "all"; model.search = "" }
        }.privateScreen().navigationTitle("Filters")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
    }
}

struct VaultRow: View {
    let item: VaultItem
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    var body: some View {
        HStack(spacing: 10) {
            ObjectIcon(glyph: .vaultItem(item.kind))
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title).font(.body.weight(.medium)).lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 1)
                if item.subtitle.isEmpty { Text(vaultKindName(item.kind)).font(.caption).foregroundStyle(.secondary) }
                else { Text(item.subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(dynamicTypeSize.isAccessibilitySize ? 2 : 1) }
            }.frame(maxWidth: .infinity, alignment: .leading)
            if item.favorite { Image(systemName: "star.fill").font(.caption).foregroundStyle(Theme.forest).accessibilityLabel("Favorite") }
            if item.conflict { Image(systemName: "arrow.triangle.branch").accessibilityLabel("Sync conflicts") }
            else if !item.synced { Image(systemName: "arrow.triangle.2.circlepath").font(.caption).foregroundStyle(.secondary).accessibilityLabel("Waiting to sync") }
        }
    }
}

struct VaultEditorTarget: Identifiable {
    let item: VaultItem?
    var kind = "login"
    var favorite = false
    var id: String { item?.id ?? "new" }
}
