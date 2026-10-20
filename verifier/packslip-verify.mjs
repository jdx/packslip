#!/usr/bin/env node
// packslip-verify.mjs: verify a keyless packslip release, with no dependencies.
//
// Commit this one file into your project, read it, and run it with Node.js 20 or
// newer. It uses only node: built-ins and makes no network requests. It checks:
//
//   1. the Rekor log entry: signed entry timestamp, the envelope and certificate
//      it binds, and an inclusion proof under a checkpoint the log signed;
//   2. the Fulcio certificate: valid when the entry was logged, chained to the
//      sigstore public-good root embedded below;
//   3. the DSSE signature over the statement;
//   4. the signer: the certificate's identity and OIDC issuer, against a policy
//      you pass (never one the bundle names);
//   5. the statement, then each file you pass: signed size and SHA-256/SHA-512.
//
// It does not check the certificate's signed certificate timestamp, an RFC 3161
// timestamp, or key-signed bundles; use `packslip verify` for those. The trust
// root below is a snapshot of sigstore's public-good trusted_root.json (Fulcio
// chains and Rekor keys). When sigstore rotates a key, replace this file with a
// newer copy.
//
// Run: node packslip-verify.mjs packslip.sigstore.json \
//        --issuer https://token.actions.githubusercontent.com \
//        --identity-prefix https://github.com/owner/repo/.github/workflows/ \
//        --artifact mytool-1.2.3-linux-x64.tar.gz

import { createHash, createPublicKey, verify as cryptoVerify, X509Certificate } from 'node:crypto';
import { createReadStream, readFileSync, realpathSync } from 'node:fs';
import { basename } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

// Snapshot of https://tuf-repo-cdn.sigstore.dev targets/trusted_root.json,
// reduced to what verification reads. Certificates and keys are base64 DER.
export const TRUSTED_ROOT = {
  "tlogs": [
    {
      "baseUrl": "https://rekor.sigstore.dev",
      "keyId": "wNI9atQGlz+VWfO6LRygH4QUfY/8W4RFwiT5i5WRgB0=",
      "publicKey": "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE2G2Y+2tabdTV5BcGiBIx0a9fAFwrkBbmLSGtks4L3qX6yYY0zufBnhC8Ur/iy55GhWP/9A/bY2LhC30M9+RYtw==",
      "start": "2021-01-12T11:53:27Z"
    },
    {
      "baseUrl": "https://log2025-1.rekor.sigstore.dev",
      "keyId": "zxGZFVvd0FEmjR8WrFwMdcAJ9vtaY/QXf44Y1wUeP6A=",
      "publicKey": "MCowBQYDK2VwAyEAt8rlp1knGwjfbcXAYPYAkn0XiLz1x8O4t0YkEhie244=",
      "start": "2025-09-23T00:00:00Z"
    }
  ],
  "cas": [
    {
      "uri": "https://fulcio.sigstore.dev",
      "chain": [
        "MIIB+DCCAX6gAwIBAgITNVkDZoCiofPDsy7dfm6geLbuhzAKBggqhkjOPQQDAzAqMRUwEwYDVQQKEwxzaWdzdG9yZS5kZXYxETAPBgNVBAMTCHNpZ3N0b3JlMB4XDTIxMDMwNzAzMjAyOVoXDTMxMDIyMzAzMjAyOVowKjEVMBMGA1UEChMMc2lnc3RvcmUuZGV2MREwDwYDVQQDEwhzaWdzdG9yZTB2MBAGByqGSM49AgEGBSuBBAAiA2IABLSyA7Ii5k+pNO8ZEWY0ylemWDowOkNa3kL+GZE5Z5GWehL9/A9bRNA3RbrsZ5i0JcastaRL7Sp5fp/jD5dxqc/UdTVnlvS16an+2Yfswe/QuLolRUCrcOE2+2iA5+tzd6NmMGQwDgYDVR0PAQH/BAQDAgEGMBIGA1UdEwEB/wQIMAYBAf8CAQEwHQYDVR0OBBYEFMjFHQBBmiQpMlEk6w2uSu1KBtPsMB8GA1UdIwQYMBaAFMjFHQBBmiQpMlEk6w2uSu1KBtPsMAoGCCqGSM49BAMDA2gAMGUCMH8liWJfMui6vXXBhjDgY4MwslmN/TJxVe/83WrFomwmNf056y1X48F9c4m3a3ozXAIxAKjRay5/aj/jsKKGIkmQatjI8uupHr/+CxFvaJWmpYqNkLDGRU+9orzh5hI2RrcuaQ=="
      ],
      "start": "2021-03-07T03:20:29Z",
      "end": "2022-12-31T23:59:59.999Z"
    },
    {
      "uri": "https://fulcio.sigstore.dev",
      "chain": [
        "MIICGjCCAaGgAwIBAgIUALnViVfnU0brJasmRkHrn/UnfaQwCgYIKoZIzj0EAwMwKjEVMBMGA1UEChMMc2lnc3RvcmUuZGV2MREwDwYDVQQDEwhzaWdzdG9yZTAeFw0yMjA0MTMyMDA2MTVaFw0zMTEwMDUxMzU2NThaMDcxFTATBgNVBAoTDHNpZ3N0b3JlLmRldjEeMBwGA1UEAxMVc2lnc3RvcmUtaW50ZXJtZWRpYXRlMHYwEAYHKoZIzj0CAQYFK4EEACIDYgAE8RVS/ysH+NOvuDZyPIZtilgUF9NlarYpAd9HP1vBBH1U5CV77LSS7s0ZiH4nE7Hv7ptS6LvvR/STk798LVgMzLlJ4HeIfF3tHSaexLcYpSASr1kS0N/RgBJz/9jWCiXno3sweTAOBgNVHQ8BAf8EBAMCAQYwEwYDVR0lBAwwCgYIKwYBBQUHAwMwEgYDVR0TAQH/BAgwBgEB/wIBADAdBgNVHQ4EFgQU39Ppz1YkEZb5qNjpKFWixi4YZD8wHwYDVR0jBBgwFoAUWMAeX5FFpWapesyQoZMi0CrFxfowCgYIKoZIzj0EAwMDZwAwZAIwPCsQK4DYiZYDPIaDi5HFKnfxXx6ASSVmERfsynYBiX2X6SJRnZU84/9DZdnFvvxmAjBOt6QpBlc4J/0DxvkTCqpclvziL6BCCPnjdlIB3Pu3BxsPmygUY7Ii2zbdCdliiow=",
        "MIIB9zCCAXygAwIBAgIUALZNAPFdxHPwjeDloDwyYChAO/4wCgYIKoZIzj0EAwMwKjEVMBMGA1UEChMMc2lnc3RvcmUuZGV2MREwDwYDVQQDEwhzaWdzdG9yZTAeFw0yMTEwMDcxMzU2NTlaFw0zMTEwMDUxMzU2NThaMCoxFTATBgNVBAoTDHNpZ3N0b3JlLmRldjERMA8GA1UEAxMIc2lnc3RvcmUwdjAQBgcqhkjOPQIBBgUrgQQAIgNiAAT7XeFT4rb3PQGwS4IajtLk3/OlnpgangaBclYpsYBr5i+4ynB07ceb3LP0OIOZdxexX69c5iVuyJRQ+Hz05yi+UF3uBWAlHpiS5sh0+H2GHE7SXrk1EC5m1Tr19L9gg92jYzBhMA4GA1UdDwEB/wQEAwIBBjAPBgNVHRMBAf8EBTADAQH/MB0GA1UdDgQWBBRYwB5fkUWlZql6zJChkyLQKsXF+jAfBgNVHSMEGDAWgBRYwB5fkUWlZql6zJChkyLQKsXF+jAKBggqhkjOPQQDAwNpADBmAjEAj1nHeXZp+13NWBNa+EDsDP8G1WWg1tCMWP/WHPqpaVo0jhsweNFZgSs0eE7wYI4qAjEA2WB9ot98sIkoF3vZYdd3/VtWB5b9TNMea7Ix/stJ5TfcLLeABLE4BNJOsQ4vnBHJ"
      ],
      "start": "2022-04-13T20:06:15Z"
    }
  ]
};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/** Every failure this file raises on purpose. `code` says which check failed. */
export class VerifyError extends Error {
  constructor(code, message) {
    super(message);
    this.name = 'VerifyError';
    this.code = code;
  }
}

const fail = (code, message) => {
  throw new VerifyError(code, message);
};

// ---------------------------------------------------------------------------
// A DER reader, just enough to read the certificate's extensions
// ---------------------------------------------------------------------------

function tlv(buf, offset) {
  if (offset + 2 > buf.length) fail('bundle', 'truncated certificate');
  const tag = buf[offset];
  let length = buf[offset + 1];
  let header = 2;
  if (length & 0x80) {
    const count = length & 0x7f;
    if (count === 0 || count > 4 || offset + 2 + count > buf.length) fail('bundle', 'bad DER length');
    length = 0;
    for (let i = 0; i < count; i++) length = length * 256 + buf[offset + 2 + i];
    header += count;
  }
  const end = offset + header + length;
  if (end > buf.length) fail('bundle', 'truncated certificate');
  return { tag, body: buf.subarray(offset + header, end), end };
}

function children(buf) {
  const out = [];
  for (let offset = 0; offset < buf.length; ) {
    const node = tlv(buf, offset);
    out.push(node);
    offset = node.end;
  }
  return out;
}

function oidString(body) {
  const parts = [];
  let value = 0;
  for (const byte of body) {
    value = value * 128 + (byte & 0x7f);
    if (!(byte & 0x80)) {
      parts.push(value);
      value = 0;
    }
  }
  const first = parts.shift();
  return [Math.min(Math.floor(first / 40), 2), first - 40 * Math.min(Math.floor(first / 40), 2), ...parts].join('.');
}

/** The extensions of a DER certificate as `{ oid: DER value }`. */
function certificateExtensions(der) {
  const certificate = tlv(der, 0);
  const tbs = children(certificate.body)[0];
  const field = children(tbs.body).find((node) => node.tag === 0xa3);
  const extensions = {};
  if (field === undefined) return extensions;
  for (const extension of children(children(field.body)[0].body)) {
    const parts = children(extension.body);
    const oid = oidString(parts[0].body);
    const value = parts[parts.length - 1];
    if (value.tag !== 0x04) fail('bundle', 'bad certificate extension');
    if (oid in extensions) fail('identity', `the certificate repeats extension ${oid}`);
    extensions[oid] = value.body;
  }
  return extensions;
}

const OID_SAN = '2.5.29.17';
const OID_EKU = '2.5.29.37';
const OID_CODE_SIGNING = '1.3.6.1.5.5.7.3.3';
const OID_ISSUER_V2 = '1.3.6.1.4.1.57264.1.8';
const OID_ISSUER_V1 = '1.3.6.1.4.1.57264.1.1';
const OID_SOURCE_REPOSITORY = '1.3.6.1.4.1.57264.1.12';

/** The identity Fulcio certified: the first URI or email in the SAN, and the OIDC issuer. */
function certificateSigner(der) {
  const extensions = certificateExtensions(der);
  const eku = extensions[OID_EKU];
  if (eku === undefined || !children(tlv(eku, 0).body).some((n) => n.tag === 0x06 && oidString(n.body) === OID_CODE_SIGNING)) {
    fail('signature', 'the certificate is not a code signing certificate');
  }
  const san = extensions[OID_SAN];
  if (san === undefined) fail('identity', 'the certificate has no subject alternative name');
  const names = children(tlv(san, 0).body).filter((n) => n.tag === 0x86 || n.tag === 0x81);
  if (names.length !== 1) fail('identity', 'the certificate must name exactly one URI or email identity');
  const identity = names[0].body.toString('utf8');
  let issuer;
  if (extensions[OID_ISSUER_V2] !== undefined) {
    const value = tlv(extensions[OID_ISSUER_V2], 0);
    if (value.tag !== 0x0c) fail('identity', 'the certificate issuer is not a UTF8String');
    issuer = value.body.toString('utf8');
  } else if (extensions[OID_ISSUER_V1] !== undefined) {
    issuer = extensions[OID_ISSUER_V1].toString('utf8');
  } else {
    fail('identity', 'the certificate does not record an OIDC issuer');
  }
  let sourceRepository;
  if (extensions[OID_SOURCE_REPOSITORY] !== undefined) {
    sourceRepository = tlv(extensions[OID_SOURCE_REPOSITORY], 0).body.toString('utf8');
  }
  return { identity, issuer, sourceRepository };
}

// ---------------------------------------------------------------------------
// Signature helpers
// ---------------------------------------------------------------------------

const b64 = (text) => Buffer.from(text, 'base64');
const sha256 = (...parts) => {
  const hash = createHash('sha256');
  for (const part of parts) hash.update(part);
  return hash.digest();
};

function checkSignature(key, data, signature) {
  const type = key.asymmetricKeyType;
  if (type === 'ed25519') return cryptoVerify(null, data, key, signature);
  if (type === 'ec') {
    const hash = { 'prime256v1': 'sha256', secp384r1: 'sha384' }[key.asymmetricKeyDetails?.namedCurve];
    if (hash === undefined) fail('signature', 'unsupported elliptic curve');
    return cryptoVerify(hash, data, key, signature);
  }
  return fail('signature', `unsupported key type ${type}`);
}

const inWindow = (entry, time) =>
  time >= Date.parse(entry.start) && (entry.end === undefined || time <= Date.parse(entry.end));

// ---------------------------------------------------------------------------
// Fulcio: the certificate chain
// ---------------------------------------------------------------------------

const validAt = (cert, time) => time >= Date.parse(cert.validFrom) && time <= Date.parse(cert.validTo);

function verifyChain(root, leaf, time) {
  if (!validAt(leaf, time)) {
    fail('signature', 'the certificate was not valid when the signature was logged');
  }
  for (const authority of root.cas.filter((ca) => inWindow(ca, time))) {
    const chain = authority.chain.map((c) => new X509Certificate(b64(c)));
    let child = leaf;
    let ok = true;
    for (const parent of chain) {
      if (!parent.ca || !child.checkIssued(parent) || !child.verify(parent.publicKey)) ok = false;
      if (!validAt(parent, time)) ok = false;
      child = parent;
    }
    // The last certificate is the trust anchor: it must be its own issuer.
    if (ok && child.checkIssued(child) && child.verify(child.publicKey)) return;
  }
  return fail('signature', 'the certificate does not chain to the sigstore Fulcio root');
}

// ---------------------------------------------------------------------------
// Rekor: signed entry timestamp, checkpoint and inclusion proof
// ---------------------------------------------------------------------------

/** RFC 6962 / RFC 9162 inclusion proof. Tree sizes pass 2^32, so this uses BigInt. */
function inclusionProofHolds(leafHash, index, size, hashes, root) {
  let fn = BigInt(index);
  let sn = BigInt(size) - 1n;
  if (fn > sn) return false;
  let hash = leafHash;
  for (const sibling of hashes) {
    if (sn === 0n) return false;
    if (fn & 1n || fn === sn) {
      hash = sha256(Buffer.from([1]), sibling, hash);
      while (!(fn & 1n) && fn !== 0n) {
        fn >>= 1n;
        sn >>= 1n;
      }
    } else {
      hash = sha256(Buffer.from([1]), hash, sibling);
    }
    fn >>= 1n;
    sn >>= 1n;
  }
  return sn === 0n && hash.equals(root);
}

function verifyCheckpoint(proof, key) {
  const text = proof.checkpoint?.envelope ?? '';
  const split = text.indexOf('\n\n');
  if (split < 0) fail('signature', 'the checkpoint is not a signed note');
  const note = text.slice(0, split + 1);
  const lines = note.split('\n');
  if (lines[1] !== String(proof.treeSize) || !b64(lines[2] ?? '').equals(b64(proof.rootHash))) {
    fail('signature', 'the checkpoint does not match the inclusion proof');
  }
  for (const line of text.slice(split + 2).split('\n')) {
    if (!line.startsWith('— ')) continue;
    const signature = b64(line.split(' ')[2] ?? '');
    // The first four bytes are a key hint; the rest is the signature.
    if (signature.length > 4 && checkSignature(key, Buffer.from(note, 'utf8'), signature.subarray(4))) return;
  }
  fail('signature', 'no valid log signature on the checkpoint');
}

/**
 * Verify the log entry the bundle carries, and return the time it was logged.
 * The entry must bind this envelope and this certificate, carry a signed entry
 * timestamp, and be included in a checkpoint the log signed.
 */
function verifyLogEntry(root, bundle, envelope, leafDer) {
  const entries = bundle.verificationMaterial?.tlogEntries;
  if (!Array.isArray(entries) || entries.length === 0) fail('signature', 'the bundle has no transparency log entry');
  const entry = entries[0];
  const keyId = entry.logId?.keyId;
  const integrated = Number(entry.integratedTime);
  const log = root.tlogs.find((t) => t.keyId === keyId);
  if (log === undefined) fail('signature', 'the log entry is from a log this verifier does not trust');
  if (!Number.isSafeInteger(integrated) || !inWindow(log, integrated * 1000)) {
    fail('signature', 'the log key was not valid when the entry was logged');
  }
  const key = createPublicKey({ key: b64(log.publicKey), format: 'der', type: 'spki' });

  // The signed entry timestamp covers the canonical JSON of these four fields.
  const promise = entry.inclusionPromise?.signedEntryTimestamp;
  if (promise === undefined) fail('signature', 'the log entry has no signed entry timestamp');
  const signed = Buffer.from(
    JSON.stringify({
      body: entry.canonicalizedBody,
      integratedTime: integrated,
      logID: b64(keyId).toString('hex'),
      logIndex: Number(entry.logIndex),
    }),
  );
  if (!checkSignature(key, signed, b64(promise))) fail('signature', 'the signed entry timestamp is not valid');

  // The logged body must be this envelope's signature, payload and certificate.
  const body = JSON.parse(b64(entry.canonicalizedBody).toString('utf8'));
  if (body.kind !== 'dsse' || body.apiVersion !== '0.0.1') fail('signature', 'the log entry is not a DSSE entry');
  const payloadHash = sha256(b64(envelope.payload)).toString('hex');
  const logged = body.spec?.signatures;
  if (
    body.spec?.payloadHash?.algorithm !== 'sha256' ||
    body.spec.payloadHash.value !== payloadHash ||
    !Array.isArray(logged) ||
    logged.length !== 1 ||
    logged[0].signature !== envelope.signatures[0].sig
  ) {
    fail('signature', 'the log entry is for a different envelope');
  }
  const pem = b64(logged[0].verifier).toString('utf8');
  if (!new X509Certificate(pem).raw.equals(leafDer)) fail('signature', 'the log entry is for a different certificate');

  const proof = entry.inclusionProof;
  if (proof === undefined) fail('signature', 'the log entry has no inclusion proof');
  const leafHash = sha256(Buffer.from([0]), b64(entry.canonicalizedBody));
  if (!inclusionProofHolds(leafHash, proof.logIndex, proof.treeSize, proof.hashes.map(b64), b64(proof.rootHash))) {
    fail('signature', 'the inclusion proof does not match the log root');
  }
  verifyCheckpoint(proof, key);
  return integrated * 1000;
}

// ---------------------------------------------------------------------------
// The statement
// ---------------------------------------------------------------------------

export const RELEASE_PREDICATE_TYPE = 'https://packslip.dev/release/v1';
const isObject = (v) => typeof v === 'object' && v !== null && !Array.isArray(v);

export function readStatement(payload, expect) {
  let statement;
  try {
    statement = JSON.parse(b64(payload).toString('utf8'));
  } catch {
    return fail('statement', 'the signed payload is not valid JSON');
  }
  if (!isObject(statement) || statement._type !== 'https://in-toto.io/Statement/v1') {
    fail('statement', 'the signed payload is not an in-toto statement');
  }
  if (statement.predicateType !== RELEASE_PREDICATE_TYPE) {
    fail('predicate-type', `the predicate type is ${JSON.stringify(statement.predicateType)}, not ${RELEASE_PREDICATE_TYPE}`);
  }
  const predicate = statement.predicate;
  if (
    !isObject(predicate) ||
    typeof predicate.project !== 'string' ||
    typeof predicate.version !== 'string' ||
    !Array.isArray(predicate.artifacts)
  ) {
    fail('statement', 'the predicate needs a project, a version, and artifacts');
  }
  for (const artifact of predicate.artifacts) {
    if (
      !isObject(artifact) ||
      typeof artifact.name !== 'string' ||
      !(Number.isSafeInteger(artifact.size) && artifact.size >= 0)
    ) {
      fail('statement', 'each artifact needs a name and a non-negative integer size');
    }
  }
  if (!Array.isArray(statement.subject) || statement.subject.length === 0) fail('statement', 'the statement has no subjects');
  const seen = new Set();
  for (const subject of statement.subject) {
    const digest = isObject(subject) ? subject.digest : undefined;
    if (!isObject(subject) || typeof subject.name !== 'string' || !isObject(digest)) {
      fail('statement', 'a subject needs a name and a digest');
    }
    if (seen.has(subject.name)) fail('statement', `the statement lists ${JSON.stringify(subject.name)} twice`);
    seen.add(subject.name);
    if (typeof digest.sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(digest.sha256)) {
      fail('statement', `${subject.name}: sha256 must be 64 lowercase hexadecimal characters`);
    }
    if (digest.sha512 !== undefined && (typeof digest.sha512 !== 'string' || !/^[0-9a-f]{128}$/.test(digest.sha512))) {
      fail('statement', `${subject.name}: sha512 must be 128 lowercase hexadecimal characters`);
    }
  }
  if (expect.project !== undefined && predicate.project !== expect.project) {
    fail('project', `the statement is for ${predicate.project}, not ${expect.project}`);
  }
  if (expect.tag !== undefined) {
    const tag = isObject(predicate.source) ? predicate.source.tag : undefined;
    if (tag !== expect.tag) {
      fail('tag', `the statement's source tag is ${JSON.stringify(tag)}, not ${JSON.stringify(expect.tag)}`);
    }
  }
  return statement;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Verify a keyless release bundle against a signer you already trust.
 *
 * `identity` is the exact certificate identity, or a function that accepts one,
 * and `issuer` the exact OIDC issuer; the policy must come from you, never from
 * the bundle. `project`, `tag` and `sourceRepository` are optional further
 * checks, and `trustedRoot` replaces the embedded trust root (same shape).
 * Returns the signed statement plus `signer`.
 * Throws a `VerifyError` for every failure, including a malformed bundle.
 */
export function verifyRelease(bundleJson, options) {
  try {
    return verifyBundle(bundleJson, options);
  } catch (error) {
    if (error instanceof VerifyError) throw error;
    throw new VerifyError('bundle', `the bundle is malformed: ${error.message}`);
  }
}

function verifyBundle(bundleJson, options) {
  if (typeof options?.issuer !== 'string' || (options.identity === undefined && options.sourceRepository === undefined)) {
    fail('usage', 'a signer policy needs an issuer and an identity or a source repository');
  }
  let bundle;
  try {
    bundle = JSON.parse(typeof bundleJson === 'string' ? bundleJson : Buffer.from(bundleJson).toString('utf8'));
  } catch {
    return fail('bundle', 'the bundle is not valid JSON');
  }
  const envelope = bundle?.dsseEnvelope;
  if (!isObject(envelope) || envelope.payloadType !== 'application/vnd.in-toto+json' || typeof envelope.payload !== 'string') {
    fail('bundle', 'the bundle needs a DSSE envelope with payload type application/vnd.in-toto+json');
  }
  if (!Array.isArray(envelope.signatures) || envelope.signatures.length !== 1) {
    fail('bundle', 'the DSSE envelope must carry exactly one signature');
  }
  const raw = bundle.verificationMaterial?.certificate?.rawBytes;
  if (typeof raw !== 'string') fail('bundle', 'the bundle has no certificate; key-signed bundles need `packslip verify`');
  const leafDer = b64(raw);
  const leaf = new X509Certificate(leafDer);

  // 1. The log entry says when the signature existed, and binds it to this envelope.
  const root = options.trustedRoot ?? TRUSTED_ROOT;
  const time = verifyLogEntry(root, bundle, envelope, leafDer);
  // 2. The certificate must have been valid then and chain to Fulcio.
  verifyChain(root, leaf, time);
  // 3. The certificate's key must have signed the envelope (DSSE pre-authentication encoding).
  const payload = b64(envelope.payload);
  const type = Buffer.from(envelope.payloadType);
  const pae = Buffer.concat([Buffer.from(`DSSEv1 ${type.length} `), type, Buffer.from(` ${payload.length} `), payload]);
  if (!checkSignature(leaf.publicKey, pae, b64(envelope.signatures[0].sig))) {
    fail('signature', 'the envelope signature does not match the certificate');
  }
  // 4. The signer must be the one the caller trusts.
  const signer = certificateSigner(leafDer);
  const accepted = typeof options.identity === 'function' ? options.identity(signer.identity) : options.identity === undefined || signer.identity === options.identity;
  if (!accepted) fail('identity', `the signing identity ${signer.identity} is not the expected one`);
  if (signer.issuer !== options.issuer) {
    fail('identity', `the certificate issuer ${signer.issuer} is not the expected ${options.issuer}`);
  }
  if (options.sourceRepository !== undefined && signer.sourceRepository !== options.sourceRepository) {
    fail('identity', `the certificate's source repository is ${signer.sourceRepository}, not ${options.sourceRepository}`);
  }
  // 5. Only now read the statement.
  const statement = readStatement(envelope.payload, options);
  return { ...statement, signer: { identity: signer.identity, issuer: signer.issuer } };
}

/** Check a downloaded file against the digest the verified statement lists for its name. */
export async function verifyFile(statement, path) {
  const name = basename(path);
  const subject = statement.subject.find((s) => s.name === name);
  if (subject === undefined) fail('artifact-not-found', `the release statement does not list ${JSON.stringify(name)}`);
  const info = statement.predicate.artifacts.find((a) => a.name === name);
  const hashes = { sha256: createHash('sha256'), ...(subject.digest.sha512 ? { sha512: createHash('sha512') } : {}) };
  let size = 0;
  try {
    for await (const chunk of createReadStream(path)) {
      size += chunk.length;
      for (const hash of Object.values(hashes)) hash.update(chunk);
    }
  } catch (error) {
    fail('file', `cannot read ${path}: ${error.message}`);
  }
  if (info !== undefined && size !== info.size) {
    fail('digest-mismatch', `${name}: size is ${size}, but the signed statement says ${info.size}`);
  }
  for (const [algorithm, hash] of Object.entries(hashes)) {
    const actual = hash.digest('hex');
    if (actual !== subject.digest[algorithm]) {
      fail('digest-mismatch', `${name}: ${algorithm} is ${actual}, but the signed statement says ${subject.digest[algorithm]}`);
    }
  }
  return { name, size, ...subject.digest };
}

// ---------------------------------------------------------------------------
// Command line
// ---------------------------------------------------------------------------

const USAGE = `usage: node packslip-verify.mjs BUNDLE --issuer URL (--identity SAN | --identity-prefix PREFIX | --source-repository URL)
                                [--project HOST/OWNER/REPO] [--tag TAG] [--artifact FILE]... [--json]

Verifies a keyless packslip bundle and the files you downloaded. The policy flags
must come from you, not from the release.`;

function readBundle(path) {
  try {
    return readFileSync(path);
  } catch (error) {
    return fail('bundle', `cannot read ${path}: ${error.message}`);
  }
}

async function main(argv) {
  let args;
  try {
    args = parseArgs({
      args: argv,
      allowPositionals: true,
      options: {
        issuer: { type: 'string' },
        identity: { type: 'string' },
        'identity-prefix': { type: 'string' },
        'source-repository': { type: 'string' },
        project: { type: 'string' },
        tag: { type: 'string' },
        artifact: { type: 'string', multiple: true },
        json: { type: 'boolean' },
      },
    });
  } catch (error) {
    console.error(`${error.message}\n${USAGE}`);
    return 2;
  }
  const { values, positionals } = args;
  const policies = ['identity', 'identity-prefix', 'source-repository'].filter((name) => values[name] !== undefined);
  if (positionals.length !== 1 || values.issuer === undefined || policies.length !== 1 || values['identity-prefix'] === '') {
    console.error(USAGE);
    return 2;
  }
  try {
    const prefix = values['identity-prefix'];
    const statement = verifyRelease(readBundle(positionals[0]), {
      issuer: values.issuer,
      identity: prefix === undefined ? values.identity : (san) => san.startsWith(prefix),
      sourceRepository: values['source-repository'],
      project: values.project,
      tag: values.tag,
    });
    const files = [];
    for (const path of values.artifact ?? []) files.push(await verifyFile(statement, path));
    const { project, version } = statement.predicate;
    if (values.json) {
      console.log(JSON.stringify({ ok: true, project, version, signer: statement.signer, files }));
    } else {
      console.log(`verified ${project} ${version}, signed by ${statement.signer.identity}`);
      for (const file of files) console.log(`  ok  ${file.name}  sha256:${file.sha256}`);
    }
    return 0;
  } catch (error) {
    // Anything that is not a VerifyError is a bug here, but still a failure, not a crash.
    const code = error instanceof VerifyError ? error.code : 'error';
    if (values.json) console.log(JSON.stringify({ ok: false, code, error: error.message }));
    else console.error(`verification failed: ${error.message}`);
    return 1;
  }
}

if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  process.exitCode = await main(process.argv.slice(2));
}
