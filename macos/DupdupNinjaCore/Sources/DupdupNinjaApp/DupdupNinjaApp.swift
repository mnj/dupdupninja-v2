import SwiftUI
import AppKit
import DupdupNinjaCore

@main
struct DupdupNinjaApp: App {
    @State private var statusText: String = "Status: Idle"
    @State private var progressValue: Double = 0
    @State private var progressIndeterminate: Bool = false
    @State private var isScanning: Bool = false
    @State private var cancelToken: CancelToken?
    @State private var scanTask: Task<Void, Never>?

    @Environment(\.openWindow) private var openWindow

    var body: some Scene {
        WindowGroup {
            ContentView(
                statusText: $statusText,
                progressValue: $progressValue,
                progressIndeterminate: $progressIndeterminate,
                isScanning: $isScanning,
                onCancel: cancelScan
            )
            .frame(minWidth: 900, minHeight: 600)
        }
        .commands {
            CommandGroup(replacing: .appInfo) {
                Button("About dupdupninja") {
                    openWindow(id: "about")
                }
            }

            CommandMenu("Scan") {
                Button("Scan Folder…") {
                    if let url = pickDirectory(title: "Select a folder to scan", initial: nil) {
                        startScan(root: url)
                    }
                }

                Button("Scan Disk…") {
                    let volumes = URL(fileURLWithPath: "/Volumes", isDirectory: true)
                    if let url = pickDirectory(title: "Select a disk/mount to scan", initial: volumes) {
                        let (scanRoot, _) = resolveVolume(from: url)
                        startScan(root: scanRoot)
                    }
                }
            }
        }

        Window("About dupdupninja", id: "about") {
            AboutView()
                .frame(minWidth: 420, minHeight: 240)
        }

        Settings {
            SettingsView()
                .frame(minWidth: 520, minHeight: 360)
        }
    }

    private func startScan(root: URL) {
        if isScanning {
            statusText = "Status: Scan already running"
            return
        }

        isScanning = true
        progressIndeterminate = true
        progressValue = 0
        statusText = "Status: Preparing scan…"

        let token = CancelToken()
        cancelToken = token

        scanTask?.cancel()
        scanTask = Task {
            do {
                let totals = try await Task.detached {
                    let engine = Engine()
                    try engine.prescanFolder(rootPath: root.path, cancel: token) { progress in
                        let folder = URL(fileURLWithPath: progress.currentPath).lastPathComponent
                        let label = folder.isEmpty ? progress.currentPath : folder
                        DispatchQueue.main.async {
                            statusText = "Status: Preparing \(label) (\(progress.filesSeen) files)"
                            progressIndeterminate = true
                        }
                    }
                }.value

                await MainActor.run {
                    progressIndeterminate = false
                    progressValue = 0
                    statusText = "Status: Scanning…"
                }

                let dbPath = FileManager.default.temporaryDirectory
                    .appendingPathComponent("dupdupninja-scan-\(Int(Date().timeIntervalSince1970)).sqlite3")

                try await Task.detached {
                    let engine = Engine()
                    try engine.scanFolderToSqliteWithProgress(
                        rootPath: root.path,
                        dbPath: dbPath.path,
                        cancel: token,
                        totals: totals
                    ) { progress in
                        let folder = URL(fileURLWithPath: progress.currentPath).deletingLastPathComponent().lastPathComponent
                        let label = folder.isEmpty ? progress.currentPath : folder
                        let fraction = totals.totalFiles > 0
                            ? Double(progress.filesSeen) / Double(totals.totalFiles)
                            : 0
                        DispatchQueue.main.async {
                            statusText = "Status: Scanning \(label) (\(progress.filesSeen)/\(totals.totalFiles))"
                            progressIndeterminate = false
                            progressValue = min(max(fraction, 0), 1)
                        }
                    }
                }.value

                await MainActor.run {
                    statusText = "Status: Scan complete"
                    progressIndeterminate = false
                    progressValue = 1
                    isScanning = false
                    cancelToken = nil
                }
            } catch {
                let message = String(describing: error)
                let final = message.contains("cancelled") ? "Status: Scan cancelled" : "Status: Scan error: \(message)"
                await MainActor.run {
                    statusText = final
                    progressIndeterminate = false
                    progressValue = 0
                    isScanning = false
                    cancelToken = nil
                }
            }
        }
    }

    private func cancelScan() {
        cancelToken?.cancel()
        statusText = "Status: Cancelling…"
    }
}

private struct ContentView: View {
    @Binding var statusText: String
    @Binding var progressValue: Double
    @Binding var progressIndeterminate: Bool
    @Binding var isScanning: Bool
    let onCancel: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                Text(statusText)
                    .font(.system(size: 16, design: .monospaced))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(24)
            }

            Divider()

            HStack(spacing: 12) {
                Text(statusText)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)

                if progressIndeterminate {
                    ProgressView()
                        .frame(width: 200)
                } else {
                    ProgressView(value: progressValue)
                        .frame(width: 200)
                }

                Button("Cancel") {
                    onCancel()
                }
                .disabled(!isScanning)
            }
            .padding(12)
        }
    }
}

private struct AboutView: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("dupdupninja")
                .font(.system(size: 20, weight: .semibold))
            Text("Cross-platform duplicate/near-duplicate media finder.")
                .font(.system(size: 13))
            Text("Version \(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1.0")")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        }
        .padding(20)
    }
}

private struct SettingsView: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Settings")
                .font(.system(size: 20, weight: .semibold))
            Text("Settings are not implemented yet.")
                .font(.system(size: 13))
        }
        .padding(20)
    }
}

private struct VolumeMetadata {
    var uuid: String?
    var label: String?
    var fsType: String?
}

private func pickDirectory(title: String, initial: URL?) -> URL? {
    let panel = NSOpenPanel()
    panel.title = title
    panel.canChooseFiles = false
    panel.canChooseDirectories = true
    panel.allowsMultipleSelection = false
    panel.canCreateDirectories = false
    if let initial {
        panel.directoryURL = initial
    }
    let res = panel.runModal()
    return res == .OK ? panel.url : nil
}

private func resolveVolume(from url: URL) -> (URL, VolumeMetadata) {
    let volumeURL = (try? url.resourceValues(forKeys: [.volumeURLKey]).volume) ?? url
    let uuid = (try? volumeURL.resourceValues(forKeys: [.volumeUUIDStringKey]).volumeUUIDString)
    let label = (try? volumeURL.resourceValues(forKeys: [.volumeNameKey]).volumeName)
    let fsType = fileSystemType(for: volumeURL)
    return (volumeURL, VolumeMetadata(uuid: uuid, label: label, fsType: fsType))
}

private func fileSystemType(for url: URL) -> String? {
    var st = statfs()
    return url.path.withCString { cpath in
        if statfs(cpath, &st) != 0 {
            return nil
        }
        return withUnsafePointer(to: &st.f_fstypename) { ptr in
            ptr.withMemoryRebound(to: CChar.self, capacity: Int(MFSTYPENAMELEN)) { cstr in
                String(cString: cstr)
            }
        }
    }
}
