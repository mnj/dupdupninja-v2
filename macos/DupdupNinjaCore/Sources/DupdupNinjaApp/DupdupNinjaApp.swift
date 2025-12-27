import SwiftUI
import AppKit

@main
struct DupdupNinjaApp: App {
    @State private var statusText: String = "dupdupninja (SwiftUI skeleton)"
    @Environment(\.openWindow) private var openWindow

    var body: some Scene {
        WindowGroup {
            ContentView(statusText: $statusText)
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
                        statusText = "Folder scan path:\n\(url.path)"
                        print("scan folder: \(url.path)")
                    }
                }

                Button("Scan Disk…") {
                    let volumes = URL(fileURLWithPath: "/Volumes", isDirectory: true)
                    if let url = pickDirectory(title: "Select a disk/mount to scan", initial: volumes) {
                        let (scanRoot, meta) = resolveVolume(from: url)
                        statusText =
                            "Disk scan path:\n\(scanRoot.path)\n\n" +
                            "Disk id (UUID): \(meta.uuid ?? "(unknown)")\n" +
                            "Disk label: \(meta.label ?? "(unknown)")\n" +
                            "FS type: \(meta.fsType ?? "(unknown)")"

                        print("scan disk path: \(scanRoot.path)")
                        print("disk id: \(meta.uuid ?? "nil")")
                        print("disk label: \(meta.label ?? "nil")")
                        print("disk fs_type: \(meta.fsType ?? "nil")")
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
}

private struct ContentView: View {
    @Binding var statusText: String

    var body: some View {
        ScrollView {
            Text(statusText)
                .font(.system(size: 16, design: .monospaced))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(24)
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
