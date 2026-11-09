import AuthenticationServices
import UIKit
import DMsgCore

/// Native transport only. The shared core and auth canister verify the result.
@MainActor
final class PasskeyLogin: NSObject, ASAuthorizationControllerDelegate, ASAuthorizationControllerPresentationContextProviding {
    private var controller: ASAuthorizationController?
    private var continuation: CheckedContinuation<NativePasskeyResponse, Error>?

    func authenticate(_ start: PasskeyStart) async throws -> NativePasskeyResponse {
        cancel()
        guard start.deadlineMs > UInt64(Date.now.timeIntervalSince1970 * 1000) else {
            throw MobileError.Failure(code: "EXPIRED")
        }
        let provider = ASAuthorizationPlatformPublicKeyCredentialProvider(relyingPartyIdentifier: "dmsg.net")
        let request: ASAuthorizationRequest
        switch start.operation {
        case .register, .bind:
            let creation = provider.createCredentialRegistrationRequest(challenge: start.challenge, name: "dMsg", userID: start.userId)
            creation.userVerificationPreference = .required
            creation.attestationPreference = .none
            request = creation
        case .login, .logoutAll:
            let assertion = provider.createCredentialAssertionRequest(challenge: start.challenge)
            assertion.userVerificationPreference = .required
            if let id = start.credentialId {
                assertion.allowedCredentials = [ASAuthorizationPlatformPublicKeyCredentialDescriptor(credentialID: id)]
            }
            request = assertion
        }
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                self.continuation = continuation
                let controller = ASAuthorizationController(authorizationRequests: [request])
                controller.delegate = self
                controller.presentationContextProvider = self
                self.controller = controller
                controller.performRequests()
            }
        } onCancel: { Task { @MainActor [weak self] in self?.cancel() } }
    }

    func cancel() {
        let old = controller
        controller = nil
        let pending = continuation
        continuation = nil
        old?.cancel()
        pending?.resume(throwing: CancellationError())
    }

    func authorizationController(controller: ASAuthorizationController, didCompleteWithAuthorization authorization: ASAuthorization) {
        guard controller === self.controller else { return }
        let result: NativePasskeyResponse
        if let registration = authorization.credential as? ASAuthorizationPlatformPublicKeyCredentialRegistration,
           let attestation = registration.rawAttestationObject {
            result = NativePasskeyResponse(credentialId: registration.credentialID,
                clientDataJson: registration.rawClientDataJSON, authenticatorData: attestation, signature: Data())
        } else if let assertion = authorization.credential as? ASAuthorizationPlatformPublicKeyCredentialAssertion {
            result = NativePasskeyResponse(credentialId: assertion.credentialID,
                clientDataJson: assertion.rawClientDataJSON, authenticatorData: assertion.rawAuthenticatorData,
                signature: assertion.signature)
        } else {
            finish(.failure(MobileError.Failure(code: "INTEGRITY_FAILED")))
            return
        }
        finish(.success(result))
    }

    func authorizationController(controller: ASAuthorizationController, didCompleteWithError error: Error) {
        guard controller === self.controller else { return }
        finish(.failure((error as? ASAuthorizationError)?.code == .canceled ? CancellationError() : error))
    }

    private func finish(_ result: Result<NativePasskeyResponse, Error>) {
        let pending = continuation
        continuation = nil
        controller = nil
        pending?.resume(with: result)
    }

    func presentationAnchor(for controller: ASAuthorizationController) -> ASPresentationAnchor {
        UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows).first(where: \.isKeyWindow) ?? ASPresentationAnchor()
    }
}
