# dMsg (ICPanda Message) Design Choices

## CBOR

The Concise Binary Object Representation ([CBOR, RFC 8949](https://datatracker.ietf.org/doc/html/rfc8949)) is a data format whose design goals include the possibility of extremely small code size, fairly small message size, and extensibility without the need for version negotiation. These design goals make it different from earlier binary serializations such as ASN.1 and MessagePack.

The Internet Computer source code also makes extensive use of CBOR. All dMsg smart contracts and the client use CBOR as the primary data serialization format.

## COSE

COSE ([RFC 9052](https://datatracker.ietf.org/doc/html/rfc9052), [RFC 9053](https://datatracker.ietf.org/doc/html/rfc9053)) is a standard for signing and encrypting data in the CBOR data format. It is designed to be simple and efficient, and to be usable in constrained environments. It is intended to be used in a variety of applications, including the Internet of Things, and is designed to be extensible to support new algorithms and applications.

dMsg uses COSE as the standard for message encryption and key exchange.

## IC-COSE

[IC-COSE](https://github.com/ldclabs/ic-cose) is a decentralized COnfiguration service with Signing and Encryption on the Internet Computer.

After registering a username, dMsg users gain a dedicated namespace on the COSE service for key derivation, key storage, and other confidential information.

## IC-OSS

[IC-OSS](https://github.com/ldclabs/ic-oss) is a decentralized Object Storage Service on the Internet Computer.

dMsg uses IC-OSS to store user avatars, channel logos, and channel files. Each channel has a dedicated folder on the OSS service for file storage.
