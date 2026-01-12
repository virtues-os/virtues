//
//  NetworkManager.swift
//  Virtues
//
//  Handles all network communication with retry logic
//

import Foundation
import Combine

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
    let success: Bool
    let taskId: String?  // Optional since Celery processing may be disabled
    let pipelineActivityId: String
    let dataSizeBytes: Int
    let dataSize: String
    let source: String
    let message: String
    let streamKey: String
    
    private enum CodingKeys: String, CodingKey {
        case success
        case taskId = "task_id"
        case pipelineActivityId = "pipeline_activity_id"
        case dataSizeBytes = "data_size_bytes"
        case dataSize = "data_size"
        case source
        case message
        case streamKey = "stream_key"
    }
}

struct ErrorResponse: Codable {
    let error: String
    let details: String?
}