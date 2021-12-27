import * as assert from "assert";
import * as anchor from "@project-serum/anchor";
import { BN, Program, web3 } from "@project-serum/anchor";

import { Multisig } from "../target/types/multisig.js";
import { AccountMeta, LAMPORTS_PER_SOL } from "@solana/web3.js";

const provider = anchor.Provider.env();
anchor.setProvider(provider);
const program = anchor.workspace.Multisig as Program<Multisig>;
const wallet = program.provider.wallet;

const base = anchor.web3.Keypair.generate();
const ownerA = anchor.web3.Keypair.generate();
const ownerB = anchor.web3.Keypair.generate();
const ownerC = anchor.web3.Keypair.generate();
let key, bump;

describe("multisig", () => {
  before(async () => {
    [key, bump] = await pda(['multisig', base.publicKey]);
    await airdrop(base.publicKey);
    await airdrop(ownerA.publicKey);
    await airdrop(ownerB.publicKey);
    await airdrop(ownerC.publicKey);
  });

  it("createMultisig", async () => {
    const owners = [ownerA.publicKey, ownerB.publicKey, ownerC.publicKey];
    await program.rpc.createMultisig(owners, bn(2, 0), bn(0), bump, {
      accounts: {
        signer: wallet.publicKey,
        base: base.publicKey,
        multisig: key,
        systemProgram: anchor.web3.SystemProgram.programId,
      },
    });

    const multisig = await program.account.multisig.fetch(key);
    assert.equal(multisig.bump, bump);
    assert.equal(multisig.numTransactions.toNumber(), 0);
    assert.equal(multisig.threshold.toNumber(), 2);
    assert.deepEqual(multisig.owners, owners);
  });

  it("setOwners", async () => {
    let multisig = await program.account.multisig.fetch(key);
    const ix = program.instruction.setOwners([ownerA.publicKey], {
      accounts: {
        multisig: key
      },
    });
    const [txKey, txBump] = await pda(['transaction', key, multisig.numTransactions.toNumber()]);

    // Can't create when now an owner
    try {
      await program.rpc.createTransaction([ix], txBump, {
        accounts: {
          signer: wallet.publicKey,
          multisig: key,
          transaction: txKey,
          systemProgram: web3.SystemProgram.programId,
        },
      });
      throw new Error('did not throw');
    } catch (err) {
      assert.ok(err.message.includes('owner is not part'));
    }

    // Create setOwners transaction
    await program.rpc.createTransaction([ix], txBump, {
      accounts: {
        signer: ownerA.publicKey,
        multisig: key,
        transaction: txKey,
        systemProgram: web3.SystemProgram.programId,
      },
      signers: [ownerA],
    });

    let tx = await program.account.transaction.fetch(txKey);

    assert.equal(tx.instructions[0].programId.toString(), ix.programId.toString());
    assert.equal(tx.instructions[0].data.toString(), ix.data.toString());
    assert.ok(tx.multisig.equals(key));
    assert.deepEqual(tx.proposer, ownerA.publicKey);
    assert.equal(tx.executedAt.toString(), '0');

    multisig = await program.account.multisig.fetch(key);
    assert.equal(multisig.numTransactions.toNumber(), 1);

    const remainingAccounts = ix.keys
      .map(k => Object.assign(k, { isSigner: false })).concat([{
        pubkey: program.programId,
        isSigner: false,
        isWritable: false,
      }]);

    // Can't execute before threshold is reached
    try {
      await program.rpc.executeTransaction({
        accounts: {
          signer: ownerA.publicKey,
          multisig: key,
          transaction: txKey,
        },
        remainingAccounts,
        signers: [ownerA],
      });
      throw new Error('did not throw');
    } catch (err) {
      assert.match(err.message, /Not enough owners signed/);
    }

    // Approve transaction as ownerB to reach threshold
    await program.rpc.approve({
      accounts: {
        signer: ownerB.publicKey,
        multisig: key,
        transaction: txKey,
      },
      signers: [ownerB],
    });

    // Execute transaction, owner will be updated after
    await program.rpc.executeTransaction({
      accounts: {
        signer: ownerC.publicKey,
        multisig: key,
        transaction: txKey,
      },
      remainingAccounts,
      signers: [ownerC],
    });

    // Can't execute a transaction twice
    try {
      await program.rpc.executeTransaction({
        accounts: {
          signer: ownerA.publicKey,
          multisig: key,
          transaction: txKey,
        },
        remainingAccounts,
        signers: [ownerA],
      });
      throw new Error('did not throw');
    } catch (err) {
      assert.ok(err.message.includes('has already been executed'));
    }

    multisig = await program.account.multisig.fetch(key);
    assert.equal(multisig.threshold.toNumber(), 1);
    assert.deepEqual(multisig.owners, [ownerA.publicKey]);

    tx = await program.account.transaction.fetch(txKey);
    assert.equal(tx.executor.toString(), ownerC.publicKey.toString());
    assert.notEqual(tx.executedAt.toNumber(), 0);
  });

  it('changeThreshold', async () => {
    const ix = program.instruction.changeThreshold(bn(1, 0), {
      accounts: {
        multisig: key
      },
    });
    const txKey = await createApproveExecute(ix);
    const multisig = await program.account.multisig.fetch(key);
    assert.equal(multisig.threshold.toNumber(), 1);
  });

  it('changeDelay', async () => {
    const ix = program.instruction.changeDelay(bn(60, 0), {
      accounts: {
        multisig: key
      },
    });
    const txKey = await createApproveExecute(ix);
    const multisig = await program.account.multisig.fetch(key);
    assert.equal(multisig.delay.toNumber(), 60);
  });
});

async function createApproveExecute(ix) {
  let multisig = await program.account.multisig.fetch(key);
  const [txKey, txBump] = await pda(['transaction', key, multisig.numTransactions.toNumber()]);
  await program.rpc.createTransaction([ix], txBump, {
    accounts: {
      signer: ownerA.publicKey,
      multisig: key,
      transaction: txKey,
      systemProgram: web3.SystemProgram.programId,
    },
    signers: [ownerA],
  });

  const remainingAccounts = ix.keys
    .map(k => Object.assign(k, { isSigner: false })).concat([{
      pubkey: program.programId,
      isSigner: false,
      isWritable: false,
    }])
  await program.rpc.executeTransaction({
    accounts: {
      signer: ownerA.publicKey,
      multisig: key,
      transaction: txKey,
    },
    remainingAccounts,
    signers: [ownerA],
  });
  return txKey;
}

async function airdrop(key) {
  const tx = await program.provider.connection.requestAirdrop(
    key,
    LAMPORTS_PER_SOL
  );
  await program.provider.connection.confirmTransaction(tx);
}

async function pda(seeds, programId = program.programId) {
  for (let i = 0; i < seeds.length; i++) {
    if (typeof seeds[i] === "number") {
      const a = Buffer.from(new BN(seeds[i]).toArray().reverse());
      const b = Buffer.alloc(8);
      a.copy(b);
      seeds[i] = b;
    }
    if (typeof seeds[i] === "string") {
      seeds[i] = Buffer.from(seeds[i]);
    }
    if (typeof seeds[i].toBuffer == "function") {
      seeds[i] = seeds[i].toBuffer();
    }
  }
  return await web3.PublicKey.findProgramAddress(seeds, programId);
}

function bn(value, decimals = 9) {
  return new BN(value).mul(new BN(10).pow(new BN(decimals)));
}
