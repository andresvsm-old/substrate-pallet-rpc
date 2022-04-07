#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_support::{
		traits::{tokens::ExistenceRequirement, Currency},
	};
	use frame_system::pallet_prelude::*;
	use scale_info::TypeInfo;
	use sp_std::prelude::*;

	#[cfg(feature = "std")]
	use serde::{Deserialize, Serialize};

	type ProductId = Vec<u8>;
	type H256 = Vec<u8>;
	type Timestamp = Vec<u8>;
	type AccountOf<T> = <T as frame_system::Config>::AccountId;
	type BalanceOf<T> =
		<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	// Struct for the actual Item saved on-chain
	#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct Item<T: Config> {
		pub id: ProductId,
		pub summary: H256,
		pub owner: AccountOf<T>,
		pub state: State,
		pub price: Option<BalanceOf<T>>,
		pub last_update: Timestamp,
		pub creation_block: BlockNumberFor<T>,
	}

	// Enum for Item's State
	#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
	pub enum State {
		ForSale,
		Sold,
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// The Currency handler for the Items pallet.
		type Currency: Currency<Self::AccountId>;
	}

	// Errors.
	#[pallet::error]
	pub enum Error<T> {
		/// Checks if an Id already exists
		ProductIdExists,
		/// Handles the correctness of the item
		ItemHasChanged,
		/// Ensures the item is for sale
		ItemNotForSale,
		/// Buyer cannot be the owner.
		BuyerIsItemSeller,
		/// Ensures the ownership of the item
		NotItemOwner,
		/// Ensures the item exists
		ItemNotFound,
		/// Ensures the amount sent for a purchase is enough
		AmountTooLow,
		/// Ensures an account has enough free balance
		NotEnoughBalance,
		/// Ensures the item affected is 'finalized' (block finalized)
		ItemIsNotFinalized,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new Item was sucessfully created. \[sender, item_id, creation_block\]
		Created(T::AccountId, ProductId, BlockNumberFor<T>),
		/// An Item was sucessfully transferred. \[from, to, item_id\]
		Transferred(T::AccountId, T::AccountId, ProductId),
		/// An Item was sucessfully bought. \[buyer, seller, item_id, amount, execution_block\]
		Bought(T::AccountId, T::AccountId, ProductId, BalanceOf<T>, BlockNumberFor<T>),
	}

	// Storage items.

	#[pallet::storage]
	#[pallet::getter(fn items)]
	/// 'Mapping' Item ID -> Item Data
	pub(super) type Items<T: Config> = StorageMap<_, Twox64Concat, ProductId, Item<T>>;

	// Our pallet's genesis configuration.
	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub items: Vec<(T::AccountId, ProductId, H256, BalanceOf<T>, Timestamp)>,
	}

	// Required to implement default for GenesisConfig.
	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> GenesisConfig<T> {
			GenesisConfig { items: vec![] }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			// When building an item from genesis config, we require some information of the item to be provided
			for (acct, id, summary, price, timestamp) in &self.items {
				let _ = <Pallet<T>>::mint(
					acct,
					id.clone(),
					summary.clone(),
					price.clone(),
					timestamp.clone(),
				);
			}
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new unique item.
		///
		/// The actual item creation is done in the `mint()` function.
		#[pallet::weight(100)]
		pub fn create_item(
			origin: OriginFor<T>,
			id: ProductId,
			summary: H256,
			price: BalanceOf<T>,
			timestamp: Timestamp,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			ensure!(
				!<Items<T>>::contains_key(&id),
				Error::<T>::ProductIdExists
			);
			let (item_reference, creation_block) =
				Self::mint(&sender, id, summary, price, timestamp)?;

			// Logging to the console
			log::info!("🎈 An Item with ID ➡ {:?} has been created.", item_reference);
			// Deposit our "Created" event.
			Self::deposit_event(Event::Created(sender, item_reference, creation_block));
			Ok(())
		}

		/// Buy a `for sale` item. The amount provided from the buyer has to be equal to the price of the item
		#[pallet::weight(100)]
		pub fn buy_item(
			origin: OriginFor<T>,
			id: ProductId,
			amount: BalanceOf<T>,
			timestamp: Timestamp,
		) -> DispatchResult {
			let buyer = ensure_signed(origin)?;
			// Check the item exists and buyer is not the current item owner
			let item = Self::items(&id).ok_or(<Error<T>>::ItemNotFound)?;
			ensure!(item.owner != buyer, <Error<T>>::BuyerIsItemSeller);
			ensure!(
				item.creation_block < <frame_system::Pallet<T>>::block_number(),
				<Error<T>>::ItemIsNotFinalized
			);
			// Check the item is for sale and the item ask price <= amount
			if let Some(ask_price) = item.price {
				ensure!(ask_price == amount, <Error<T>>::AmountTooLow);
			} else {
				Err(<Error<T>>::ItemNotForSale)?;
			}

			// Check the buyer has enough free balance
			ensure!(T::Currency::free_balance(&buyer) >= amount, <Error<T>>::NotEnoughBalance);

			let seller = item.owner.clone();

			// Transfer the amount from buyer to seller
			T::Currency::transfer(&buyer, &seller, amount, ExistenceRequirement::KeepAlive)?;

			// Transfer the item from seller to buyer
			Self::transfer_item_to(&id, &buyer, &timestamp)?;

			Self::deposit_event(Event::Bought(
				buyer,
				seller,
				id,
				amount,
				<frame_system::Pallet<T>>::block_number(),
			));

			Ok(())
		}
	}

	//** Our helper functions.**//

	impl<T: Config> Pallet<T> {
		// Helper to mint a Item.
		pub fn mint(
			owner: &T::AccountId,
			id: ProductId,
			summary: H256,
			price: BalanceOf<T>,
			timestamp: Timestamp,
		) -> Result<(ProductId, BlockNumberFor<T>), Error<T>> {
			let creation_block = <frame_system::Pallet<T>>::block_number();
			let item = Item::<T> {
				id: id.clone(),
				summary,               // Item resumend info
				state: State::ForSale, // On creation, the item is for sale
				owner: owner.clone(),  // Who owns the item i.e the seller
				price: Some(price),    // Price, which is actually not included in the previous hash
				last_update: timestamp,
				creation_block,
			};
			// let item_ref = T::Hashing::hash_of(&id);
			<Items<T>>::insert(&id, item);

			Ok((id, creation_block))
		}

		pub fn transfer_item_to(
			id: &[u8],
			to: &T::AccountId,
			timestamp: &[u8],
		) -> Result<(), Error<T>> {
			let mut item = Self::items(id).ok_or(<Error<T>>::ItemNotFound)?;

			// Update the item owner
			item.owner = to.clone();
			// Remove price
			item.price = None;
			// Change state
			item.state = State::Sold;
			// Update timestamp
			item.last_update = (*timestamp).to_vec();

			<Items<T>>::insert(id, item);

			Ok(())
		}

		pub fn get_sum() -> u32 {
			2
		}
	}
}
