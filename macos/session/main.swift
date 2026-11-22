// Runs the shared session against this Mac's Bluetooth for a few seconds and prints every
// snapshot it reports, one JSON object a line. What the probe is to IOBluetooth, this is to the
// bridge.
//
//     macos/run.sh session                          # for ten seconds
//     macos/run.sh session 30 '{"set_volume": 12}'  # for thirty, sending that halfway through
//
// With OHC_TRACE=1 in the environment, every message to and from the headset is printed too.

import Foundation

setvbuf(stdout, nil, _IOLBF, 0)
let arguments = Array(CommandLine.arguments.dropFirst())
let seconds = arguments.first.flatMap(Double.init) ?? 10

HeadsetSession.start { snapshot in
    print(snapshot)
}
if arguments.count > 1 {
    // Halfway, so that the session has heard what the headset can do: it drops what it cannot.
    DispatchQueue.main.asyncAfter(deadline: .now() + seconds / 2) {
        print(HeadsetSession.send(arguments[1]) ? "> \(arguments[1])" : "not a command: \(arguments[1])")
    }
}
RunLoop.main.run(until: Date().addingTimeInterval(seconds))
