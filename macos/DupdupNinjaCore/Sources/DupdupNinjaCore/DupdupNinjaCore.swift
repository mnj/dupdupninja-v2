import Foundation
import CDupdupNinja

public enum DupdupNinjaError: Error, CustomStringConvertible {
    case ffiError(String)

    public var description: String {
        switch self {
        case .ffiError(let msg): return msg
        }
    }
}

public struct ScanTotals: Sendable {
    public let totalFiles: UInt64
    public let totalBytes: UInt64
}

public struct PrescanProgress: Sendable {
    public let filesSeen: UInt64
    public let bytesSeen: UInt64
    public let dirsSeen: UInt64
    public let currentPath: String
}

public struct ScanProgress: Sendable {
    public let filesSeen: UInt64
    public let filesHashed: UInt64
    public let filesSkipped: UInt64
    public let bytesSeen: UInt64
    public let totalFiles: UInt64
    public let totalBytes: UInt64
    public let currentPath: String
}

public final class CancelToken: @unchecked Sendable {
    private let ptr: UnsafeMutablePointer<DupdupCancelToken>

    public init() {
        self.ptr = dupdupninja_cancel_token_new()
    }

    deinit {
        dupdupninja_cancel_token_free(ptr)
    }

    public func cancel() {
        dupdupninja_cancel_token_cancel(ptr)
    }

    fileprivate var raw: UnsafeMutablePointer<DupdupCancelToken> {
        ptr
    }
}

public final class Engine {
    private let ptr: UnsafeMutablePointer<DupdupEngine>

    public init() {
        self.ptr = dupdupninja_engine_new()
    }

    deinit {
        dupdupninja_engine_free(ptr)
    }

    public func scanFolderToSqlite(rootPath: String, dbPath: String) throws {
        let status = rootPath.withCString { rootC in
            dbPath.withCString { dbC in
                dupdupninja_scan_folder_to_sqlite(ptr, rootC, dbC)
            }
        }
        guard status == DUPDUP_STATUS_OK else {
            throw lastErrorOrUnknown()
        }
    }

    public func prescanFolder(
        rootPath: String,
        cancel: CancelToken,
        onProgress: @escaping (PrescanProgress) -> Void
    ) throws -> ScanTotals {
        let callback: @convention(c) (UnsafePointer<DupdupPrescanProgress>?, UnsafeMutableRawPointer?) -> Void =
            { progressPtr, userData in
                guard let progressPtr, let userData else { return }
                let progress = progressPtr.pointee
                let handler = Unmanaged<PrescanHandler>.fromOpaque(userData).takeUnretainedValue()
                let path = progress.current_path != nil ? String(cString: progress.current_path) : ""
                handler.callback(PrescanProgress(
                    filesSeen: progress.files_seen,
                    bytesSeen: progress.bytes_seen,
                    dirsSeen: progress.dirs_seen,
                    currentPath: path
                ))
            }

        let handler = PrescanHandler(callback: onProgress)
        let unmanaged = Unmanaged.passRetained(handler)
        defer { unmanaged.release() }

        var totals = DupdupPrescanTotals(total_files: 0, total_bytes: 0)
        let status = rootPath.withCString { rootC in
            dupdupninja_prescan_folder(
                rootC,
                cancel.raw,
                callback,
                unmanaged.toOpaque(),
                &totals
            )
        }

        guard status == DUPDUP_STATUS_OK else {
            throw lastErrorOrUnknown()
        }

        return ScanTotals(totalFiles: totals.total_files, totalBytes: totals.total_bytes)
    }

    public func scanFolderToSqliteWithProgress(
        rootPath: String,
        dbPath: String,
        cancel: CancelToken,
        totals: ScanTotals,
        onProgress: @escaping (ScanProgress) -> Void
    ) throws {
        let callback: @convention(c) (UnsafePointer<DupdupProgress>?, UnsafeMutableRawPointer?) -> Void =
            { progressPtr, userData in
                guard let progressPtr, let userData else { return }
                let progress = progressPtr.pointee
                let handler = Unmanaged<ScanHandler>.fromOpaque(userData).takeUnretainedValue()
                let path = progress.current_path != nil ? String(cString: progress.current_path) : ""
                handler.callback(ScanProgress(
                    filesSeen: progress.files_seen,
                    filesHashed: progress.files_hashed,
                    filesSkipped: progress.files_skipped,
                    bytesSeen: progress.bytes_seen,
                    totalFiles: progress.total_files,
                    totalBytes: progress.total_bytes,
                    currentPath: path
                ))
            }

        let handler = ScanHandler(callback: onProgress)
        let unmanaged = Unmanaged.passRetained(handler)
        defer { unmanaged.release() }

        let status = rootPath.withCString { rootC in
            dbPath.withCString { dbC in
                dupdupninja_scan_folder_to_sqlite_with_progress_and_totals(
                    ptr,
                    rootC,
                    dbC,
                    cancel.raw,
                    totals.totalFiles,
                    totals.totalBytes,
                    callback,
                    unmanaged.toOpaque()
                )
            }
        }

        guard status == DUPDUP_STATUS_OK else {
            throw lastErrorOrUnknown()
        }
    }

    private func lastErrorOrUnknown() -> DupdupNinjaError {
        if let cmsg = dupdupninja_last_error_message() {
            return DupdupNinjaError.ffiError(String(cString: cmsg))
        }
        return DupdupNinjaError.ffiError("unknown error")
    }
}

private final class PrescanHandler {
    let callback: (PrescanProgress) -> Void
    init(callback: @escaping (PrescanProgress) -> Void) {
        self.callback = callback
    }
}

private final class ScanHandler {
    let callback: (ScanProgress) -> Void
    init(callback: @escaping (ScanProgress) -> Void) {
        self.callback = callback
    }
}
