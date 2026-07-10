import Foundation

#if canImport(FinanceKit)
  import FinanceKit
#endif

// Shared outbox enqueue (defined in the reach plugin's ffi.rs).
@_silgen_name("virtues_enqueue")
private func virtues_enqueue(_ stream: UnsafePointer<CChar>, _ json: UnsafePointer<CChar>) -> Int32

private let iso: ISO8601DateFormatter = {
  let f = ISO8601DateFormatter()
  f.formatOptions = [.withInternetDateTime]
  return f
}()

/// FinanceKit collector: Apple Wallet accounts + the last 3 years of
/// transactions → shared outbox → box. The box expects wrapper records
/// (`{accounts:[…]}` / `{transactions:[…]}`) and dedups inner items by their
/// Apple ids, so re-syncs are safe. FinanceKit is iOS 17.4+ and only returns
/// data when Apple Card / Apple Cash / connected accounts exist.
public final class FinanceCollector {
  public static let shared = FinanceCollector()

  private var collecting = false
  private let enabledKey = "virtues.finance.enabled"
  private let backYears = 3

  /// FinanceKit APIs **abort the app** if called without the
  /// `com.apple.developer.financekit` entitlement. It's now signed in (the
  /// entitlement value must be the array ["financial-data"], not `true`).
  private let entitled = true

  private init() {}

  public var isCollecting: Bool { collecting }

  /// Opt-in flag (FinanceKit doesn't expose a simple read-auth accessor).
  public func authorized() -> Bool { UserDefaults.standard.bool(forKey: enabledKey) }

  public func enable(_ completion: @escaping (Bool) -> Void) {
    NSLog("[Finance] enable() called, entitled=%d", entitled ? 1 : 0)
    guard entitled else { completion(false); return }
    #if canImport(FinanceKit)
      guard #available(iOS 17.4, *) else {
        NSLog("[Finance] iOS < 17.4, unsupported")
        completion(false)
        return
      }
      NSLog("[Finance] checking isDataAvailable…")
      guard FinanceStore.isDataAvailable(.financialData) else {
        NSLog("[Finance] isDataAvailable == false")
        completion(false)
        return
      }
      NSLog("[Finance] isDataAvailable == true, spawning auth task")
      Task {
        let ok = await self.requestAndCollect()
        DispatchQueue.main.async {
          if ok {
            UserDefaults.standard.set(true, forKey: self.enabledKey)
            self.collecting = true
          }
          completion(ok)
        }
      }
    #else
      completion(false)
    #endif
  }

  public func resume() {
    guard entitled else { return }
    #if canImport(FinanceKit)
      guard #available(iOS 17.4, *), authorized() else { return }
      collecting = true
      Task { await self.collect() }
    #endif
  }

  public func collectAll() {
    guard entitled else { return }
    #if canImport(FinanceKit)
      guard #available(iOS 17.4, *), authorized() else { return }
      Task { await self.collect() }
    #endif
  }

  #if canImport(FinanceKit)
    @available(iOS 17.4, *)
    private func requestAndCollect() async -> Bool {
      do {
        NSLog("[Finance] calling requestAuthorization()…")
        let status = try await FinanceStore.shared.requestAuthorization()
        NSLog("[Finance] requestAuthorization returned: %@", "\(status)")
        guard status == .authorized else { return false }
        NSLog("[Finance] authorized, starting collect()")
        await collect()
        NSLog("[Finance] collect() finished")
        return true
      } catch {
        NSLog("[Finance] auth failed: %@", error.localizedDescription)
        return false
      }
    }

    @available(iOS 17.4, *)
    private func collect() async {
      let store = FinanceStore.shared

      // Accounts → one wrapper record.
      if let accounts = try? await store.accounts(query: AccountQuery()) {
        let arr: [[String: Any]] = accounts.map { acct in
          [
            "id": acct.id.uuidString,
            "name": acct.displayName,
            "institutionName": acct.institutionName,
            "currencyCode": acct.currencyCode,
          ]
        }
        if !arr.isEmpty { enqueue(["accounts": arr]) }
      }

      // Transactions (last N years) → wrapper records, chunked.
      let start = Calendar.current.date(byAdding: .year, value: -backYears, to: Date()) ?? Date.distantPast
      let predicate = #Predicate<FinanceKit.Transaction> { $0.transactionDate >= start }
      let query = TransactionQuery(
        sortDescriptors: [SortDescriptor(\.transactionDate)],
        predicate: predicate
      )
      if let txns = try? await store.transactions(query: query) {
        var batch: [[String: Any]] = []
        for t in txns {
          let amount = NSDecimalNumber(decimal: t.transactionAmount.amount).doubleValue
          var rec: [String: Any] = [
            "id": t.id.uuidString,
            "amount": amount,
            "currencyCode": t.transactionAmount.currencyCode,
            "date": iso.string(from: t.transactionDate),
            "accountId": t.accountID.uuidString,
            "status": "\(t.status)",
            "description": t.transactionDescription,
          ]
          if let m = t.merchantName { rec["merchantName"] = m }
          batch.append(rec)
          if batch.count >= 500 {
            enqueue(["transactions": batch])
            batch = []
          }
        }
        if !batch.isEmpty { enqueue(["transactions": batch]) }
      }
    }
  #endif

  private func enqueue(_ rec: [String: Any]) {
    guard
      let data = try? JSONSerialization.data(withJSONObject: rec),
      let json = String(data: data, encoding: .utf8)
    else { return }
    let rc = "financekit".withCString { s in json.withCString { j in virtues_enqueue(s, j) } }
    if rc != 0 { NSLog("[Finance] enqueue failed rc=%d", rc) }
  }
}
