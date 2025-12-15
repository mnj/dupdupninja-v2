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
            if let cmsg = dupdupninja_last_error_message() {
                throw DupdupNinjaError.ffiError(String(cString: cmsg))
            }
            throw DupdupNinjaError.ffiError("unknown error")
        }
    }
}
