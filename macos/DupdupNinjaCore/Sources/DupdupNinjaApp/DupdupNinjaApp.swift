import SwiftUI

@main
struct DupdupNinjaApp: App {
    @State private var statusText: String = "dupdupninja (SwiftUI skeleton)"

    var body: some Scene {
        WindowGroup {
            ContentView(statusText: $statusText)
                .frame(minWidth: 900, minHeight: 600)
        }
        .commands {
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

