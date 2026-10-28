import AuthenticationServices
import UIKit

/// A service's sign-in in Apple's sign-in sheet, over the app.
///
/// Handing a sign-in to Safari sent the app to the background, and a sign-in
/// that returns to the box through the app's listener at 127.0.0.1:7117 never
/// got through: Safari could not load that address. In the sheet the app stays
/// in front. The sheet is Safari underneath, so it shares Safari's sign-ins,
/// and Google accepts it where it refuses an in-app web view.
///
/// A sign-in's last page closes the sheet by sending it to `scheme`: the OAuth
/// proxy's connection return, and the box's `/oauth/callback` for a source,
/// both do when the sign-in was started from here.
final class SignInSheet: NSObject, ASWebAuthenticationPresentationContextProviding {
  static let shared = SignInSheet()

  /// Only this sheet listens for it; the app does not register it with iOS.
  static let scheme = "virtues-signin"

  /// Kept so the session outlives `open`. One sign-in at a time.
  private var session: ASWebAuthenticationSession?
  /// The window the sheet shows over.
  private var window: UIWindow?

  /// Shows the sheet over the window `window` returns, read on the main
  /// thread. `done` gets `finished` when the sign-in's last page closed it,
  /// `closed` when the owner did, and `failed` when it could not open.
  func open(_ url: URL, over window: @escaping () -> UIWindow?, done: @escaping (String) -> Void) {
    DispatchQueue.main.async {
      self.session?.cancel()
      self.window = window()
      let session = ASWebAuthenticationSession(url: url, callbackURLScheme: Self.scheme) {
        callback, error in
        self.session = nil
        if callback != nil {
          done("finished")
        } else if let e = error as? ASWebAuthenticationSessionError, e.code == .canceledLogin {
          done("closed")
        } else {
          done("failed")
        }
      }
      session.presentationContextProvider = self
      // Share Safari's cookies, so a service the owner is signed in to in
      // Safari does not ask again.
      session.prefersEphemeralWebBrowserSession = false
      self.session = session
      if !session.start() {
        self.session = nil
        done("failed")
      }
    }
  }

  func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
    window ?? ASPresentationAnchor()
  }
}
