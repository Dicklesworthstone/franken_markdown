import SwiftUI
import UniformTypeIdentifiers

struct MarkdownResourcePanel: View {
    @ObservedObject var renderer: MarkdownRendererModel
    @State private var kind: MarkdownResourceKind = .image
    @State private var destination = ""
    @State private var fontSlot: MarkdownFontSlot = .bodyRegular
    @State private var importerIsPresented = false
    @State private var selection: Selection?
    @State private var failure: String?

    private struct Selection {
        let kind: MarkdownResourceKind
        let key: String
        let owner: UUID
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Images & fonts").font(.headline)
            Text("Attach the files this document uses. Their bytes stay with this local draft and are included in supported previews and exports.")
                .font(.caption).foregroundStyle(.secondary)
            Picker("Attachment", selection: $kind) {
                Text("Image").tag(MarkdownResourceKind.image)
                Text("Font").tag(MarkdownResourceKind.font)
            }
            .pickerStyle(.segmented)
            .accessibilityIdentifier("resource-kind")
            if kind == .image {
                TextField("Image destination, e.g. images/chart.png", text: $destination)
                    .textFieldStyle(.roundedBorder)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .accessibilityIdentifier("resource-destination")
                Text("Match the destination in ![description](destination). Leave blank to use the selected filename. For collections, use the resolved chapter-relative path.")
                    .font(.caption).foregroundStyle(.secondary)
            } else {
                Picker("Font slot", selection: $fontSlot) {
                    ForEach(MarkdownFontSlot.allCases) { slot in Text(slot.title).tag(slot) }
                }
                .accessibilityIdentifier("resource-font-slot")
            }
            Button {
                selection = Selection(
                    kind: kind, key: kind == .image ? destination : fontSlot.rawValue,
                    owner: renderer.resourceDocumentOwner
                )
                importerIsPresented = true
            } label: {
                Label(kind == .image ? "Attach image…" : "Attach font…", systemImage: "paperclip")
            }
            .buttonStyle(.bordered)
            .disabled(renderer.resourcesAreLoading || !renderer.draftRecoveryIsComplete)
            .accessibilityIdentifier("resource-import")
            if renderer.resourcesAreLoading { ProgressView("Preparing attachments…") }
            if let status = renderer.resourceStatus {
                Text(status).font(.caption).foregroundStyle(.secondary)
                    .accessibilityIdentifier("resource-status")
            }
            if renderer.hasUnavailableResources {
                Button("Discard unavailable attachment set", role: .destructive) {
                    let owner = renderer.resourceDocumentOwner
                    Task {
                        do { try await renderer.discardUnavailableResources(owner: owner); failure = nil }
                        catch is CancellationError { }
                        catch { failure = error.localizedDescription }
                    }
                }
                .disabled(renderer.resourcesAreLoading)
            }
            if let failure {
                Label(failure, systemImage: "exclamationmark.triangle.fill")
                    .font(.caption).foregroundStyle(.red)
            }
            if !renderer.resources.entries.isEmpty {
                Divider()
                ForEach(renderer.resources.entries) { attachment in
                    HStack(alignment: .top, spacing: 10) {
                        Image(systemName: attachment.kind == .image ? "photo" : "textformat")
                        VStack(alignment: .leading, spacing: 3) {
                            Text(attachment.key).font(.subheadline).textSelection(.enabled)
                            Text("\(attachment.filename) · \(byteCount(attachment.bytes.count))")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button(role: .destructive) {
                            let owner = renderer.resourceDocumentOwner
                            Task {
                                do { try await renderer.removeResource(id: attachment.id, owner: owner); failure = nil }
                                catch is CancellationError { }
                                catch { failure = error.localizedDescription }
                            }
                        } label: { Image(systemName: "minus.circle") }
                        .buttonStyle(.plain)
                        .disabled(renderer.resourcesAreLoading)
                        .accessibilityLabel("Remove attachment \(attachment.key)")
                    }
                }
                Text("\(byteCount(renderer.resources.byteCount)) of 32 MiB · Up to 16 MiB per file")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Text("Attachments are copied into this app’s draft recovery. Saving Markdown writes the source text; exported HTML, PDF, SVG and EPUB carry their supported resources.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .fileImporter(
            isPresented: $importerIsPresented,
            allowedContentTypes: selectedContentTypes,
            allowsMultipleSelection: false
        ) { result in
            guard let selection else { return }
            self.selection = nil
            guard selection.owner == renderer.resourceDocumentOwner else { return }
            switch result {
            case let .success(urls):
                guard let url = urls.first else { return }
                Task {
                    do {
                        try await renderer.importResource(
                            from: url, kind: selection.kind, key: selection.key, owner: selection.owner
                        )
                        failure = nil
                    } catch is CancellationError { }
                    catch { failure = error.localizedDescription }
                }
            case let .failure(error): failure = error.localizedDescription
            }
        }
        .onChange(of: renderer.resourceDocumentOwner) { _, _ in failure = nil }
    }

    private var selectedContentTypes: [UTType] {
        if (selection?.kind ?? kind) == .image { return [.png, .jpeg, .gif, .svg] }
        return [UTType(filenameExtension: "ttf") ?? .data, UTType(filenameExtension: "otf") ?? .data]
    }

    private func byteCount(_ value: Int) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(value), countStyle: .file)
    }
}
