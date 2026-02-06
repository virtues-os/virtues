//
//  NetworkManager.swift
//  Virtues
//
//  Handles all network communication with retry logic
//

import Foundation
import Combine
import UIKit

enum NetworkError: LocalizedError {
    case invalidURL
    case invalidToken              // 401 - requires re-auth
    case serverError(Int)          // 5xx - transient, retry with backoff
    case timeout                   // transient, retry
    case noConnection              // transient, retry when online
    case decodingError
    case rateLimited(retryAfter: TimeInterval)  // 429 - back off, don't break circuit
    case badRequest(message: String)            // 400 - permanent fail, don't retry
    case forbidden                              // 403 - permanent fail, don't retry
    case unknown(Error)

    var errorDescription: String? {
        switch self {
        case .invalidURL:
            return "Invalid API endpoint URL"
        case .invalidToken:
            return "Invalid device token (E002)"
        case .serverError(let code):
            return "Server error: \(code) (E003)"
        case .timeout:
            return "Network timeout (E001)"
        case .noConnection:
            return "No internet connection"
        case .decodingError:
            return "Failed to decode response"
        case .rateLimited(let retryAfter):
            return "Rate limited - retry after \(Int(retryAfter))s (E004)"
        case .badRequest(let message):
            return "Bad request: \(message) (E005)"
        case .forbidden:
            return "Access forbidden (E006)"
        case .unknown(let error):
            return "Unknown error: \(error.localizedDescription)"
        }
    }

    var errorCode: String {
        switch self {
        case .timeout: return "E001"
        case .invalidToken: return "E002"
        case .serverError: return "E003"
        case .rateLimited: return "E004"
        case .badRequest: return "E005"
        case .forbidden: return "E006"
        default: return "E000"
        }
    }

    /// Whether this error should be retried
    var isRetryable: Bool {
        switch self {
        case .serverError, .timeout, .noConnection, .rateLimited:
            return true
        case .invalidToken, .badRequest, .forbidden, .invalidURL, .decodingError, .unknown:
            return false
        }
    }

    /// Whether this error should count toward circuit breaker
    var countsTowardCircuitBreaker: Bool {
        switch self {
        case .serverError, .timeout:
            return true
        case .rateLimited, .badRequest, .forbidden, .invalidToken, .noConnection, .invalidURL, .decodingError, .unknown:
            return false
        }
    }
}

class NetworkManager: ObservableObject {
    static let shared = NetworkManager()
    
    private let session: URLSession
    private let timeout: TimeInterval = 30.0
    private var cancellables = Set<AnyCancellable>()
    
    @Published var isConnected: Bool = true
    @Published var lastError: NetworkError?
    
    private init() {
        let configuration = URLSessionConfiguration.default
        configuration.timeoutIntervalForRequest = timeout
        configuration.timeoutIntervalForResource = timeout
        configuration.waitsForConnectivity = true
        configuration.allowsCellularAccess = true
        
        self.session = URLSession(configuration: configuration)
    }
    
    // MARK: - Data Upload
    
    func uploadData<T: Encodable>(_ data: T, deviceToken: String, endpoint: URL) async throws -> UploadResponse {
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(deviceToken)", forHTTPHeaderField: "Authorization")
        
        // Encode data
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        request.httpBody = try encoder.encode(data)
        
        do {
            let (data, response) = try await session.data(for: request)
            
            guard let httpResponse = response as? HTTPURLResponse else {
                throw NetworkError.unknown(NSError(domain: "Invalid response", code: 0))
            }
            
            switch httpResponse.statusCode {
            case 200...299:
                return try JSONDecoder().decode(UploadResponse.self, from: data)
            case 401:
                throw NetworkError.invalidToken
            case 400:
                // Parse error message from response if available
                let message: String
                if let errorResponse = try? JSONDecoder().decode(ErrorResponse.self, from: data) {
                    message = errorResponse.error
                } else if let bodyString = String(data: data, encoding: .utf8) {
                    message = bodyString
                } else {
                    message = "Invalid request data"
                }
                throw NetworkError.badRequest(message: message)
            case 403:
                throw NetworkError.forbidden
            case 429:
                // Parse Retry-After header or use default
                let retryAfter: TimeInterval
                if let retryAfterHeader = httpResponse.value(forHTTPHeaderField: "Retry-After"),
                   let seconds = TimeInterval(retryAfterHeader) {
                    retryAfter = seconds
                } else {
                    retryAfter = 60  // Default to 60 seconds
                }
                throw NetworkError.rateLimited(retryAfter: retryAfter)
            case 500...599:
                throw NetworkError.serverError(httpResponse.statusCode)
            default:
                throw NetworkError.unknown(NSError(domain: "HTTP \(httpResponse.statusCode)", code: httpResponse.statusCode))
            }
        } catch {
            if let urlError = error as? URLError {
                switch urlError.code {
                case .timedOut:
                    throw NetworkError.timeout
                case .notConnectedToInternet, .networkConnectionLost:
                    throw NetworkError.noConnection
                default:
                    throw NetworkError.unknown(error)
                }
            }
            
            if error is NetworkError {
                throw error
            }
            
            throw NetworkError.unknown(error)
        }
    }
    
    // MARK: - QR Pairing

    /// Complete QR-based pairing: sends device identity to the server using the
    /// source_id obtained from the scanned QR code.
    func completePairing(
        endpoint: String,
        sourceId: String,
        deviceId: String
    ) async throws -> PairingCompleteResponse {
        // Build URL: {endpoint}/api/devices/pairing/{sourceId}/complete
        let baseURL = endpoint.hasSuffix("/") ? String(endpoint.dropLast()) : endpoint
        // Strip /api suffix if present so we can build the canonical path
        let root = baseURL.hasSuffix("/api") ? String(baseURL.dropLast(4)) : baseURL

        guard let url = URL(string: "\(root)/api/devices/pairing/\(sourceId)/complete") else {
            throw NetworkError.invalidURL
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.timeoutInterval = 15.0

        // Capture device info on MainActor (UIDevice properties are MainActor-isolated)
        let deviceName = await UIDevice.current.name
        let osVersion = await UIDevice.current.systemVersion
        let appVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String

        let deviceInfo = PairingDeviceInfo(
            device_id: deviceId,
            device_name: deviceName,
            device_model: Self.modelIdentifier,
            os_version: osVersion,
            app_version: appVersion
        )

        let body = PairingCompleteRequest(device_id: deviceId, device_info: deviceInfo)
        let encoder = JSONEncoder()
        request.httpBody = try encoder.encode(body)

        let (data, response) = try await session.data(for: request)

        guard let httpResponse = response as? HTTPURLResponse else {
            throw NetworkError.unknown(NSError(domain: "Invalid response", code: 0))
        }

        switch httpResponse.statusCode {
        case 200...299:
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            return try decoder.decode(PairingCompleteResponse.self, from: data)
        case 400:
            let message: String
            if let errorResponse = try? JSONDecoder().decode(ErrorResponse.self, from: data) {
                message = errorResponse.error
            } else {
                message = "Pairing session expired or already claimed"
            }
            throw NetworkError.badRequest(message: message)
        case 404:
            throw NetworkError.badRequest(message: "Pairing session not found. The QR code may be invalid.")
        case 500...599:
            throw NetworkError.serverError(httpResponse.statusCode)
        default:
            throw NetworkError.unknown(NSError(domain: "HTTP \(httpResponse.statusCode)", code: httpResponse.statusCode))
        }
    }

    /// Hardware model identifier (e.g. "iPhone16,1")
    private static var modelIdentifier: String {
        var systemInfo = utsname()
        uname(&systemInfo)
        let mirror = Mirror(reflecting: systemInfo.machine)
        return mirror.children.reduce("") { identifier, element in
            guard let value = element.value as? Int8, value != 0 else { return identifier }
            return identifier + String(UnicodeScalar(UInt8(value)))
        }
    }

    // MARK: - Connection Test

    func testConnection(endpoint: String) async -> Bool {
        guard let url = URL(string: endpoint) else { return false }
        
        var request = URLRequest(url: url)
        request.httpMethod = "HEAD"
        request.timeoutInterval = 5.0
        
        do {
            let (_, response) = try await session.data(for: request)
            if let httpResponse = response as? HTTPURLResponse {
                return (200...499).contains(httpResponse.statusCode)
            }
        } catch {
            // Log error but don't throw
            print("Connection test failed: \(error)")
        }
        
        return false
    }
}

// MARK: - Request/Response Models

struct UploadResponse: Codable {
    let accepted: Int
    let rejected: Int
    let nextCheckpoint: String?
    let activityId: String
    
    // Optional legacy fields to prevent decoding errors if server sends them
    let success: Bool?
    let message: String?
    
    private enum CodingKeys: String, CodingKey {
        case accepted
        case rejected
        case nextCheckpoint = "next_checkpoint"
        case activityId = "activity_id"
        case success
        case message
    }
}

struct ErrorResponse: Codable {
    let error: String
    let details: String?
    let message: String? // Added to match backend
}

// MARK: - QR Pairing Models

struct PairingDeviceInfo: Codable {
    let device_id: String
    let device_name: String
    let device_model: String
    let os_version: String
    let app_version: String?
}

struct PairingCompleteRequest: Codable {
    let device_id: String
    let device_info: PairingDeviceInfo
}

struct PairingCompleteResponse: Codable {
    let sourceId: String
    let deviceToken: String
}