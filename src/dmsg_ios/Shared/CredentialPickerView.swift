import SwiftUI

struct CredentialPickerView: View {
    @Bindable var picker: CredentialPickerSession
    let cancel: () -> Void
    @FocusState private var searching: Bool
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        VStack(spacing: 0) {
            header
            switch picker.page {
            case .list:
                searchField
                results
            case let .confirmation(entry):
                confirmation(entry)
            case let .message(message):
                ScrollView {
                    VStack(spacing: 16) {
                        Text(verbatim: message).accessibilityIdentifier("autofill.message")
                        if !picker.configuring {
                            Button("Retry", action: picker.retry).buttonStyle(.bordered)
                                .accessibilityIdentifier("autofill.authenticate")
                        }
                    }
                    .frame(maxWidth: .infinity).padding(24)
                }
            case .loading, .finished:
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Color(uiColor: NativeTheme.paper).ignoresSafeArea())
        .tint(Color(uiColor: NativeTheme.forest))
    }

    private var header: some View {
        HStack(spacing: 10) {
            if case .confirmation = picker.page {
                Button(action: picker.back) {
                    Image(systemName: "chevron.backward").font(.body.weight(.semibold))
                        .frame(minWidth: 44, minHeight: 44).contentShape(Rectangle())
                }
                .accessibilityLabel(Text("Back")).accessibilityIdentifier("autofill.back")
            }
            VStack(alignment: .leading, spacing: 2) {
                if case .confirmation = picker.page {
                    Text("Fill this login?")
                } else if picker.configuring {
                    Text("Password AutoFill")
                } else {
                    Text("Choose an account")
                    if let destination = picker.destination {
                        Text(verbatim: destination).font(.subheadline).foregroundStyle(.secondary).lineLimit(2)
                    }
                }
            }
            .font(.title2.weight(.semibold)).accessibilityAddTraits(.isHeader)
            Spacer(minLength: 0)
            Button(action: cancel) {
                Image(systemName: picker.configuring ? "checkmark" : "xmark")
                    .font(.body.weight(.semibold)).foregroundStyle(Color(uiColor: .secondaryLabel))
                    .frame(width: 32, height: 32).background(Color(uiColor: .tertiarySystemFill), in: Circle())
                    .frame(minWidth: 44, minHeight: 44).contentShape(Rectangle())
            }
            .accessibilityLabel(picker.configuring ? Text("Done") : Text("Cancel"))
            .accessibilityIdentifier("autofill.close")
            .disabled(isFinished)
        }
        .padding(.leading, 20).padding(.trailing, 12).padding(.top, 8).padding(.bottom, 4)
    }

    private var isFinished: Bool {
        if case .finished = picker.page { return true }
        return false
    }

    private var searchField: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").foregroundStyle(.secondary).accessibilityHidden(true)
            TextField("Search your vault", text: $picker.query)
                .textInputAutocapitalization(.never).autocorrectionDisabled().submitLabel(.search)
                .focused($searching).onSubmit { searching = false }
                .accessibilityIdentifier("autofill.search")
            if !picker.query.isEmpty {
                Button {
                    picker.query = ""
                } label: {
                    Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                        .frame(minWidth: 44, minHeight: 44)
                }.accessibilityLabel(Text("Clear search"))
            }
        }
        .padding(.leading, 12).padding(.trailing, 4)
        .frame(minHeight: 44)
        .background(Color(uiColor: .tertiarySystemFill), in: RoundedRectangle(cornerRadius: NativeTheme.radius))
        .padding(.horizontal, 16).padding(.bottom, 6)
    }

    @ViewBuilder private var results: some View {
        let sections = picker.sections
        if sections.isEmpty {
            ContentUnavailableView {
                Label(picker.query.isEmpty ? "No items" : "No matching results", systemImage: "magnifyingglass")
            }
        } else {
            List {
                ForEach(sections, id: \.title) { section in
                    if let title = section.title {
                        Section { rows(section.entries) } header: { Text(verbatim: title).textCase(nil) }
                    } else {
                        rows(section.entries)
                    }
                }
            }
            .listStyle(.plain).scrollContentBackground(.hidden)
            .contentMargins(.vertical, 0, for: .scrollContent)
            .scrollDismissesKeyboard(.interactively)
            .accessibilityIdentifier("autofill.credentials")
        }
    }

    private func rows(_ entries: [CredentialExport]) -> some View {
        let separatorInset: CGFloat = typeSize.isAccessibilitySize ? 0 : 48
        return ForEach(entries) { entry in
            Button {
                searching = false
                picker.choose(entry)
            } label: {
                HStack(spacing: 12) {
                    if !typeSize.isAccessibilitySize { monogram(entry) }
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: entry.title).font(.body.weight(.medium)).foregroundStyle(.primary)
                        let site = entry.origin.hasPrefix("https://") ? String(entry.origin.dropFirst(8)) : entry.origin
                        let detail = [entry.username, site == entry.title ? "" : site].filter { !$0.isEmpty }.joined(separator: " · ")
                        if !detail.isEmpty {
                            Text(verbatim: detail).font(.subheadline).foregroundStyle(.secondary)
                        }
                    }.lineLimit(typeSize.isAccessibilitySize ? nil : 1)
                }
                .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .listRowInsets(EdgeInsets(top: 5, leading: 20, bottom: 5, trailing: 16))
            .listRowBackground(Color.clear)
            .alignmentGuide(.listRowSeparatorLeading) { _ in separatorInset }
            .accessibilityIdentifier("autofill.login.\(entry.itemID ?? entry.id)")
        }
    }

    private func monogram(_ entry: CredentialExport) -> some View {
        Text(verbatim: String((entry.title.isEmpty ? entry.username : entry.title).prefix(1)).uppercased())
            .font(.headline).foregroundStyle(Color(uiColor: NativeTheme.forest))
            .frame(width: 36, height: 36)
            .background(Color("Fill"), in: RoundedRectangle(cornerRadius: NativeTheme.radius, style: .continuous))
            .accessibilityHidden(true)
    }

    private func confirmation(_ entry: CredentialExport) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                VStack(alignment: .leading, spacing: 12) {
                    HStack(spacing: 12) {
                        if !typeSize.isAccessibilitySize { monogram(entry) }
                        VStack(alignment: .leading, spacing: 2) {
                            Text(verbatim: entry.title).font(.headline)
                            if !entry.username.isEmpty { Text(verbatim: entry.username) }
                            if !entry.origin.isEmpty { Text(verbatim: entry.origin).font(.subheadline).foregroundStyle(.secondary) }
                        }
                    }
                    Divider()
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Use this password for:").font(.subheadline).foregroundStyle(.secondary)
                        Text(verbatim: picker.destination ?? String(localized: "Unidentified app or website"))
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading).padding(16)
                .background(Color(uiColor: .secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: NativeTheme.radius))
                VStack(spacing: 6) {
                    Button { picker.confirm(remember: false) } label: { Text("Fill once").frame(maxWidth: .infinity) }
                        .buttonStyle(.borderedProminent).accessibilityIdentifier("autofill.once")
                    if picker.canRemember(entry) {
                        Button { picker.confirm(remember: true) } label: { Text("Fill and remember").frame(maxWidth: .infinity) }
                            .buttonStyle(.bordered).accessibilityIdentifier("autofill.remember")
                    }
                }.controlSize(.large)
            }
            .padding(.horizontal, 20).padding(.vertical, 8)
        }
    }
}
