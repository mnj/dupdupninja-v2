import Foundation
import CDupdup

public enum DupdupError: Error, CustomStringConvertible {
    case ffiError(String)

    public var description: String {
        switch self {
        case .ffiError(let msg): return msg
        }
    }
}

public final class Engine {
    private let ptr: UnsafeMutablePointer<DupdupEngine>

    public init() {
        self.ptr = dupdup_engine_new()
    }

    deinit {
        dupdup_engine_free(ptr)
    }

    public func scanFolderToSqlite(rootPath: String, dbPath: String) throws {
        let status = rootPath.withCString { rootC in
            dbPath.withCString { dbC in
                dupdup_scan_folder_to_sqlite(ptr, rootC, dbC)
            }
        }
        guard status == DUPDUP_STATUS_OK else {
            if let cmsg = dupdup_last_error_message() {
                throw DupdupError.ffiError(String(cString: cmsg))
            }
            throw DupdupError.ffiError("unknown error")
        }
    }
}

