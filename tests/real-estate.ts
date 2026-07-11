import * as anchor from "@coral-xyz/anchor";
import { BN, Program } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createAccount,
  createMint,
  getAccount,
  mintTo,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { expect } from "chai";
import os from "os";
import path from "path";
import { RealEstate } from "../target/types/real_estate";

const METADATA_PROGRAM_ID = new PublicKey("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

describe("real-estate program", () => {
  if (!process.env.ANCHOR_WALLET) {
    process.env.ANCHOR_WALLET = path.join(os.homedir(), ".config", "solana", "id.json");
  }

  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  provider.connection = new Connection(provider.connection.rpcEndpoint, "confirmed");
  provider.opts.commitment = "confirmed";
  const connection = provider.connection;
  const program = anchor.workspace.realEstate as Program<RealEstate>;
  let listingCounter = 0;

  const getNextListingId = () => {
    listingCounter += 1;
    return new BN(listingCounter);
  };

  const findListingPda = (authority: PublicKey, listingId: BN) =>
    PublicKey.findProgramAddressSync(
      [Buffer.from("listing"), authority.toBuffer(), listingId.toArrayLike(Buffer, "le", 8)],
      program.programId,
    )[0];

  const findPropertyMintPda = (listing: PublicKey) =>
    PublicKey.findProgramAddressSync([Buffer.from("mint"), listing.toBuffer()], program.programId)[0];

  const findMetadataPda = (propertyMint: PublicKey) =>
    PublicKey.findProgramAddressSync(
      [Buffer.from("metadata"), METADATA_PROGRAM_ID.toBuffer(), propertyMint.toBuffer()],
      program.programId,
    )[0];

  const findRentalVaultPda = (listing: PublicKey) =>
    PublicKey.findProgramAddressSync([Buffer.from("rental_vault"), listing.toBuffer()], program.programId)[0];

  const findPositionPda = (investor: PublicKey, listing: PublicKey) =>
    PublicKey.findProgramAddressSync(
      [Buffer.from("position"), investor.toBuffer(), listing.toBuffer()],
      program.programId,
    )[0];

  const getInvestorPropertyAta = (mint: PublicKey, owner: PublicKey) =>
    anchor.utils.token.associatedAddress({ mint, owner });

  const investInListing = async (
    fixture: Awaited<ReturnType<typeof setupFixture>>,
    tokenAmount: BN,
  ) => {
    const investorPropertyAta = await getInvestorPropertyAta(fixture.propertyMint, fixture.investor.publicKey);

    await program.methods
      .invest(tokenAmount)
      .accounts({
        investor: fixture.investor.publicKey,
        listing: fixture.listing,
        investorUsdcAccount: fixture.investorUsdcAta,
        escrowVault: fixture.escrowVault,
        investorPropertyTokenAccount: investorPropertyAta,
        propertyMint: fixture.propertyMint,
        investorPosition: findPositionPda(fixture.investor.publicKey, fixture.listing),
        usdcMint: fixture.usdcMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([fixture.investor])
      .rpc();

    return investorPropertyAta;
  };

  const releaseEscrowForListing = async (fixture: Awaited<ReturnType<typeof setupFixture>>) => {
    await program.methods
      .releaseEscrow()
      .accounts({
        authority: fixture.authority,
        listing: fixture.listing,
        escrowVault: fixture.escrowVault,
        authorityUsdcAccount: fixture.authorityUsdcAta,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
  };

  const airdrop = async (wallet: PublicKey) => {
    await connection.requestAirdrop(wallet, 2_000_000_000);
  };

  const expectProgramError = async (fn: () => Promise<unknown>, expectedCode: string) => {
    try {
      await fn();
      throw new Error("Expected instruction to fail");
    } catch (error: any) {
      const anchorError = error as anchor.AnchorError;
      expect(anchorError.error?.errorCode?.code ?? error.message).to.equal(expectedCode);
    }
  };

  const getDeadline = async (offsetSeconds = 60) => {
    const farFutureBase = 4_102_444_800; // 2100-01-01T00:00:00Z
    return new BN(farFutureBase + offsetSeconds);
  };

  const setupFixture = async () => {
    const authoritySigner = provider.wallet.payer;
    const authority = authoritySigner.publicKey;
    const investor = Keypair.generate();
    await airdrop(investor.publicKey);

    const usdcMint = await createMint(connection, provider.wallet.payer, authority, authority, 6);
    const authorityUsdcAta = await createAccount(connection, provider.wallet.payer, usdcMint, authority);
    const investorUsdcAta = await createAccount(connection, provider.wallet.payer, usdcMint, investor.publicKey);

    await mintTo(connection, provider.wallet.payer, usdcMint, authorityUsdcAta, authority, 10_000_000_000);
    await mintTo(connection, provider.wallet.payer, usdcMint, investorUsdcAta, authority, 10_000_000_000);

    const listingId = getNextListingId();
    const listing = findListingPda(authority, listingId);
    const propertyMint = findPropertyMintPda(listing);
    const metadata = findMetadataPda(propertyMint);
    const escrowVault = await anchor.utils.token.associatedAddress({ mint: usdcMint, owner: listing });
    const rentalVault = findRentalVaultPda(listing);

    const createListingArgs = {
      listingId,
      name: "Test Property",
      symbol: "TP",
      uri: "https://example.com/metadata.json",
      totalTokens: new BN(1_000),
      tokenPriceUsdc: new BN(100),
      minInvestmentTokens: new BN(10),
      raiseTarget: new BN(100_000),
      raiseDeadline: await getDeadline(),
      unitThresholdTokens: new BN(100),
      rentalYieldBps: 1000,
      isOffPlan: false,
      showExactAmounts: true,
    };

    await program.methods
      .createListing(
        createListingArgs.listingId,
        createListingArgs.name,
        createListingArgs.symbol,
        createListingArgs.uri,
        createListingArgs.totalTokens,
        createListingArgs.tokenPriceUsdc,
        createListingArgs.minInvestmentTokens,
        createListingArgs.raiseTarget,
        createListingArgs.raiseDeadline,
        createListingArgs.unitThresholdTokens,
        createListingArgs.rentalYieldBps,
        createListingArgs.isOffPlan,
        createListingArgs.showExactAmounts,
      )
      .accounts({
        listing,
        propertyMint,
        metadata,
        escrowVault,
        rentalVault,
        usdcMint,
        authority,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        metadataProgram: METADATA_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    return {
      authoritySigner,
      authority,
      investor,
      usdcMint,
      authorityUsdcAta,
      investorUsdcAta,
      listing,
      propertyMint,
      metadata,
      escrowVault,
      rentalVault,
      listingId,
      createListingArgs,
    };
  };

  it("creates a listing with valid input", async () => {
    const fixture = await setupFixture();
    const listingAccount = await program.account.propertyListing.fetch(fixture.listing);

    expect(listingAccount.authority.toBase58()).to.equal(fixture.authority.toBase58());
    expect(listingAccount.status).to.deep.equal({ fundraising: {} });
    expect(listingAccount.totalTokens.toNumber()).to.equal(1_000);
    expect(listingAccount.isVisible).to.equal(true);
  });

  it("rejects invalid create-listing parameters", async () => {
    const authority = provider.wallet.publicKey;
    const usdcMint = await createMint(connection, provider.wallet.payer, authority, authority, 6);

    const listingId = getNextListingId();
    const listing = findListingPda(authority, listingId);
    const propertyMint = findPropertyMintPda(listing);
    const metadata = findMetadataPda(propertyMint);
    const escrowVault = await anchor.utils.token.associatedAddress({ mint: usdcMint, owner: listing });
    const rentalVault = findRentalVaultPda(listing);
    const deadline = await getDeadline();

    await expectProgramError(
      () =>
        program.methods
          .createListing(
            listingId,
            "Bad Listing",
            "BL",
            "https://example.com/metadata.json",
            new BN(100),
            new BN(100),
            new BN(10),
            new BN(5_000),
            deadline,
            new BN(50),
            1000,
            false,
            true,
          )
          .accounts({
            listing,
            propertyMint,
            metadata,
            escrowVault,
            rentalVault,
            usdcMint,
            authority,
            tokenProgram: TOKEN_PROGRAM_ID,
            associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
            metadataProgram: METADATA_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: anchor.web3.SYSVAR_RENT_PUBKEY,
          })
          .rpc(),
      "InvalidAmount",
    );
  });

  it("rejects investments below the minimum threshold", async () => {
    const fixture = await setupFixture();

    await expectProgramError(
      () =>
        program.methods
          .invest(new BN(5))
          .accounts({
            investor: fixture.investor.publicKey,
            listing: fixture.listing,
            investorUsdcAccount: fixture.investorUsdcAta,
            escrowVault: fixture.escrowVault,
            investorPropertyTokenAccount: anchor.utils.token.associatedAddress({
              mint: fixture.propertyMint,
              owner: fixture.investor.publicKey,
            }),
            propertyMint: fixture.propertyMint,
            investorPosition: findPositionPda(fixture.investor.publicKey, fixture.listing),
            usdcMint: fixture.usdcMint,
            tokenProgram: TOKEN_PROGRAM_ID,
            associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
          })
          .signers([fixture.investor])
          .rpc(),
      "BelowMinimum",
    );
  });

  it("allows an investor to buy tokens and transitions to funded when raise target is met", async () => {
    const fixture = await setupFixture();
    await investInListing(fixture, new BN(1_000));

    const listingAccount = await program.account.propertyListing.fetch(fixture.listing);
    const positionAccount = await program.account.investorPosition.fetch(findPositionPda(fixture.investor.publicKey, fixture.listing));

    expect(listingAccount.tokensSold.toNumber()).to.equal(1_000);
    expect(listingAccount.totalRaised.toNumber()).to.equal(100_000);
    expect(listingAccount.status).to.deep.equal({ funded: {} });
    expect(positionAccount.tokensHeld.toNumber()).to.equal(1_000);
    expect(positionAccount.usdcInvested.toNumber()).to.equal(100_000);
  });

  it("releases escrow and marks the listing active once it is funded", async () => {
    const fixture = await setupFixture();
    await investInListing(fixture, new BN(1_000));
    await releaseEscrowForListing(fixture);

    const listingAccount = await program.account.propertyListing.fetch(fixture.listing);
    expect(listingAccount.status).to.deep.equal({ active: {} });
  });

  it("allows a refund claim for a cancelled or expired fundraising listing", async () => {
    const fixture = await setupFixture();
    const investorPropertyAta = await investInListing(fixture, new BN(50));

    await program.methods
      .cancelListing()
      .accounts({
        authority: fixture.authority,
        listing: fixture.listing,
      })
      .rpc();

    await program.methods
      .claimRefund()
      .accounts({
        investor: fixture.investor.publicKey,
        listing: fixture.listing,
        escrowVault: fixture.escrowVault,
        investorUsdcAccount: fixture.investorUsdcAta,
        investorPropertyTokenAccount: investorPropertyAta,
        propertyMint: fixture.propertyMint,
        investorPosition: findPositionPda(fixture.investor.publicKey, fixture.listing),
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([fixture.investor])
      .rpc();

    const position = await program.account.investorPosition.fetch(findPositionPda(fixture.investor.publicKey, fixture.listing));
    expect(position.tokensHeld.toNumber()).to.equal(0);
    expect(position.usdcInvested.toNumber()).to.equal(0);
  });

  it("funds the rental vault and enables rental income claims", async () => {
    const fixture = await setupFixture();
    await investInListing(fixture, new BN(1_000));
    await releaseEscrowForListing(fixture);

    await program.methods
      .fundRentalVault(new BN(5_000))
      .accounts({
        authority: fixture.authority,
        listing: fixture.listing,
        authorityUsdcAccount: fixture.authorityUsdcAta,
        rentalVault: fixture.rentalVault,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    const rentalVaultAccount = await getAccount(connection, fixture.rentalVault);
    expect(rentalVaultAccount.amount.toString()).to.equal("5000");

    await program.methods
      .claimRentalIncome()
      .accounts({
        investor: fixture.investor.publicKey,
        listing: fixture.listing,
        rentalVault: fixture.rentalVault,
        investorUsdcAccount: fixture.investorUsdcAta,
        investorPosition: findPositionPda(fixture.investor.publicKey, fixture.listing),
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([fixture.investor])
      .rpc();
  });

  it("requests and approves redemption for an investor with sufficient tokens", async () => {
    const fixture = await setupFixture();
    const investorPropertyAta = await investInListing(fixture, new BN(1_000));
    await releaseEscrowForListing(fixture);

    await program.methods
      .requestRedemption()
      .accounts({
        investor: fixture.investor.publicKey,
        listing: fixture.listing,
        investorPosition: findPositionPda(fixture.investor.publicKey, fixture.listing),
      })
      .signers([fixture.investor])
      .rpc();

    await program.methods
      .approveRedemption()
      .accounts({
        authority: fixture.authority,
        listing: fixture.listing,
        investor: fixture.investor.publicKey,
        investorPosition: findPositionPda(fixture.investor.publicKey, fixture.listing),
        investorPropertyTokenAccount: investorPropertyAta,
        propertyMint: fixture.propertyMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([fixture.investor, fixture.authoritySigner])
      .rpc();

    const position = await program.account.investorPosition.fetch(findPositionPda(fixture.investor.publicKey, fixture.listing));
    expect(position.tokensHeld.toNumber()).to.equal(0);
    expect(position.redemptionRequested).to.equal(false);
  });

  it("cancels a fundraising listing and updates metadata uri", async () => {
    const fixture = await setupFixture();

    await program.methods
      .cancelListing()
      .accounts({
        authority: fixture.authority,
        listing: fixture.listing,
      })
      .rpc();

    await program.methods
      .updateMetadataUri("https://example.com/updated-metadata.json")
      .accounts({
        authority: fixture.authority,
        listing: fixture.listing,
        metadata: fixture.metadata,
        metadataProgram: METADATA_PROGRAM_ID,
      })
      .rpc();

    const listingAccount = await program.account.propertyListing.fetch(fixture.listing);
    expect(listingAccount.status).to.deep.equal({ cancelled: {} });
    expect(listingAccount.uri).to.equal("https://example.com/updated-metadata.json");
  });
});
