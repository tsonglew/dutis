import CoreServices
import Foundation

let args = CommandLine.arguments
guard args.count > 1 else {
    print("Missing argument")
    exit(1)
}

let fileType = args[1]

guard let bundleIds = LSCopyAllRoleHandlersForContentType(fileType as CFString, LSRolesMask.all)  else {
    print("Failed to fetch bundle Ids for specified filetype")
    exit(1)
}

(bundleIds.takeRetainedValue() as NSArray)
    .compactMap { bundleId -> NSArray? in
        guard let retVal = LSCopyApplicationURLsForBundleIdentifier(bundleId as! CFString, nil) else { return nil }
        return retVal.takeRetainedValue() as NSArray
    }
    .flatMap { $0 }
    .forEach { print($0) }
