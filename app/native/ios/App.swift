import UIKit
import LeptosKitBridge

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(
        _ application: UIApplication,
        configurationForConnecting session: UISceneSession,
        options: UIScene.ConnectionOptions
    ) -> UISceneConfiguration {
        let configuration = UISceneConfiguration(name: "Main", sessionRole: session.role)
        configuration.delegateClass = SceneDelegate.self
        return configuration
    }
}

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options: UIScene.ConnectionOptions
    ) {
        guard let scene = scene as? UIWindowScene else { return }

        let window = UIWindow(windowScene: scene)
        // Release floor: --mc-page mist behind the guest tree.
        window.backgroundColor = UIColor(red: 0.910, green: 0.933, blue: 0.957, alpha: 1.0)
        installController(in: window)
        self.window = window
        window.makeKeyAndVisible()
    }

    private func installController(in window: UIWindow) {
        do {
            window.rootViewController = try applicationController()
        } catch {
            let controller = UIViewController()
            let label = UILabel()
            label.text = error.localizedDescription
            label.numberOfLines = 0
            label.accessibilityIdentifier = "leptoskit-error"
            controller.view = label
            window.rootViewController = controller
        }
    }

    private func applicationController() throws -> LeptosKitApplicationController {
        guard let embedded = Bundle.main.url(forResource: "app", withExtension: "lkb"),
              let configuration = Bundle.main.url(
                  forResource: "codepush", withExtension: "json"
              ) else {
            throw CocoaError(.fileNoSuchFile)
        }

        return try LeptosKitApplicationController(
            embedded: Data(contentsOf: embedded), configuration: Data(contentsOf: configuration),
            register: GeneratedPlugins.register
        )
    }

    func sceneDidDisconnect(_ scene: UIScene) {
        (window?.rootViewController as? LeptosKitApplicationController)?.close()
        window = nil
    }
}
