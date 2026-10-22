// A small bank: structs, impl blocks, Display, error enums and `?`.
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
enum BankError {
    Insufficient { needed: u64, available: u64 },
    NoAccount(String),
    Frozen,
}

impl fmt::Display for BankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BankError::Insufficient { needed, available } => write!(f, "need {}, have {}", needed, available),
            BankError::NoAccount(name) => write!(f, "no account named {name}"),
            BankError::Frozen => write!(f, "account is frozen"),
        }
    }
}

impl std::error::Error for BankError {}

#[derive(Debug, Clone, PartialEq)]
struct Account {
    owner: String,
    balance: u64,
    frozen: bool,
}

impl Account {
    fn new(owner: String, balance: u64, frozen: bool) -> Self {
        Self { owner, balance, frozen }
    }

    fn deposit(&mut self, amount: u64) {
        self.balance += amount;
    }

    /// Takes money out, returning the new balance.
    fn withdraw(&mut self, amount: u64) -> Result<u64, BankError> {
        if self.frozen {
            return Err(BankError::Frozen);
        }
        if amount > self.balance {
            return Err(BankError::Insufficient { needed: amount, available: self.balance });
        }
        self.balance -= amount;
        Ok(self.balance)
    }

    fn is_rich(&self) -> bool {
        self.balance > 1000
    }
}

impl fmt::Display for Account {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.owner, self.balance)
    }
}

struct Bank {
    accounts: HashMap<String, Account>,
    log: Vec<String>,
}

impl Bank {
    fn new() -> Bank {
        Bank { accounts: HashMap::new(), log: Vec::new() }
    }

    fn open(&mut self, owner: &str, initial: u64) {
        let account = Account::new(owner.to_string(), initial, false);
        self.accounts.insert(owner.to_string(), account);
        self.log.push(format!("opened {}", owner));
    }

    fn transfer(&mut self, from: &str, to: &str, amount: u64) -> Result<(), BankError> {
        if !self.accounts.contains_key(to) {
            return Err(BankError::NoAccount(to.to_string()));
        }
        let source = self.accounts.get_mut(from).ok_or(BankError::NoAccount(from.to_string()))?;
        source.withdraw(amount)?;
        self.accounts.get_mut(to).unwrap().deposit(amount);
        self.log.push(format!("moved {amount} from {from} to {to}"));
        Ok(())
    }

    fn total(&self) -> u64 {
        self.accounts.values().map(|a| a.balance).sum()
    }
}

fn main() {
    let mut bank = Bank::new();
    bank.open("ada", 500);
    bank.open("bob", 50);

    match bank.transfer("ada", "bob", 200) {
        Ok(()) => println!("transfer ok"),
        Err(e) => println!("transfer failed: {}", e),
    }
    if let Err(e) = bank.transfer("bob", "ada", 1000) {
        println!("transfer failed: {e}");
    }
    if let Err(e) = bank.transfer("bob", "zed", 1) {
        println!("transfer failed: {}", e);
    }

    let mut names: Vec<&String> = bank.accounts.keys().collect();
    names.sort();
    for name in names {
        let acct = &bank.accounts[name];
        println!("{} (rich: {})", acct, acct.is_rich());
    }
    println!("total = {}", bank.total());
    for entry in &bank.log {
        println!("log: {entry}");
    }

    let mut frozen = Account::new(String::from("cy"), 10, true);
    println!("{:?}", frozen.withdraw(5));
    let copy = frozen.clone();
    println!("{}", copy == frozen);
}
