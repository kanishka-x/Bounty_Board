#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, String, Vec
};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Developer(Address),
    Bounty(u64),
    BountyCounter,
    CompanyBounties(Address),
    DeveloperBounties(Address),
}

#[derive(Clone)]
#[contracttype]
pub struct DeveloperProfile {
    pub address: Address,
    pub skills: Vec<String>,
    pub bio: String,
    pub completed_bounties: u32,
    pub rating: u32, // out of 100
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum BountyStatus {
    Open,
    Assigned,
    Submitted,
    Completed,
    Disputed,
    Cancelled,
}

#[derive(Clone)]
#[contracttype]
pub struct Bounty {
    pub id: u64,
    pub company: Address,
    pub title: String,
    pub description: String,
    pub required_skills: Vec<String>,
    pub payment_amount: i128,
    pub payment_token: Address,
    pub status: BountyStatus,
    pub assigned_developer: Option<Address>,
    pub created_at: u64,
    pub deadline: u64,
}

#[contract]
pub struct FreelanceBountyPlatform;

#[contractimpl]
impl FreelanceBountyPlatform {
    
    /// Register or update developer profile
    pub fn register_developer(
        env: Env,
        developer: Address,
        skills: Vec<String>,
        bio: String,
    ) {
        developer.require_auth();
        
        let profile = DeveloperProfile {
            address: developer.clone(),
            skills,
            bio,
            completed_bounties: 0,
            rating: 0,
        };
        
        env.storage().instance().set(&DataKey::Developer(developer), &profile);
    }
    
    /// Update developer skills
    pub fn update_skills(env: Env, developer: Address, skills: Vec<String>) {
        developer.require_auth();
        
        let mut profile: DeveloperProfile = env.storage()
            .instance()
            .get(&DataKey::Developer(developer.clone()))
            .unwrap();
        
        profile.skills = skills;
        env.storage().instance().set(&DataKey::Developer(developer), &profile);
    }
    
    /// Get developer profile
    pub fn get_developer(env: Env, developer: Address) -> Option<DeveloperProfile> {
        env.storage().instance().get(&DataKey::Developer(developer))
    }
    
    /// Create a new bounty with escrow
    pub fn create_bounty(
        env: Env,
        company: Address,
        title: String,
        description: String,
        required_skills: Vec<String>,
        payment_amount: i128,
        payment_token: Address,
        deadline: u64,
    ) -> u64 {
        company.require_auth();
        
        // Transfer tokens to contract (escrow)
        let token_client = token::Client::new(&env, &payment_token);
        token_client.transfer(&company, &env.current_contract_address(), &payment_amount);
        
        // Get or initialize bounty counter
        let bounty_id: u64 = env.storage()
            .instance()
            .get(&DataKey::BountyCounter)
            .unwrap_or(0) + 1;
        
        env.storage().instance().set(&DataKey::BountyCounter, &bounty_id);
        
        let bounty = Bounty {
            id: bounty_id,
            company: company.clone(),
            title,
            description,
            required_skills,
            payment_amount,
            payment_token,
            status: BountyStatus::Open,
            assigned_developer: None,
            created_at: env.ledger().timestamp(),
            deadline,
        };
        
        env.storage().instance().set(&DataKey::Bounty(bounty_id), &bounty);
        
        // Track company's bounties
        let mut company_bounties: Vec<u64> = env.storage()
            .instance()
            .get(&DataKey::CompanyBounties(company.clone()))
            .unwrap_or(Vec::new(&env));
        company_bounties.push_back(bounty_id);
        env.storage().instance().set(&DataKey::CompanyBounties(company), &company_bounties);
        
        bounty_id
    }
    
    /// Developer applies/gets assigned to bounty
    pub fn assign_bounty(env: Env, bounty_id: u64, developer: Address) {
        developer.require_auth();
        
        let mut bounty: Bounty = env.storage()
            .instance()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");
        
        // Check bounty is open
        assert!(bounty.status == BountyStatus::Open, "Bounty is not open");
        
        // Check developer is registered
        let _profile: DeveloperProfile = env.storage()
            .instance()
            .get(&DataKey::Developer(developer.clone()))
            .expect("Developer not registered");
        
        bounty.status = BountyStatus::Assigned;
        bounty.assigned_developer = Some(developer.clone());
        
        env.storage().instance().set(&DataKey::Bounty(bounty_id), &bounty);
        
        // Track developer's bounties
        let mut dev_bounties: Vec<u64> = env.storage()
            .instance()
            .get(&DataKey::DeveloperBounties(developer.clone()))
            .unwrap_or(Vec::new(&env));
        dev_bounties.push_back(bounty_id);
        env.storage().instance().set(&DataKey::DeveloperBounties(developer), &dev_bounties);
    }
    
    /// Developer submits work
    pub fn submit_work(env: Env, bounty_id: u64, developer: Address) {
        developer.require_auth();
        
        let mut bounty: Bounty = env.storage()
            .instance()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");
        
        // Verify developer is assigned
        match &bounty.assigned_developer {
            Some(addr) if addr == &developer => {},
            _ => panic!("Developer not assigned to this bounty"),
        }
        
        assert!(
            bounty.status == BountyStatus::Assigned,
            "Bounty is not in assigned state"
        );
        
        bounty.status = BountyStatus::Submitted;
        env.storage().instance().set(&DataKey::Bounty(bounty_id), &bounty);
    }
    
    /// Company approves work and releases payment
    pub fn approve_and_release(env: Env, bounty_id: u64, company: Address) {
        company.require_auth();
        
        let mut bounty: Bounty = env.storage()
            .instance()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");
        
        // Verify company owns bounty
        assert!(bounty.company == company, "Not authorized");
        
        assert!(
            bounty.status == BountyStatus::Submitted,
            "Work not submitted yet"
        );
        
        let developer = bounty.assigned_developer.as_ref().expect("No developer assigned");
        
        // Release payment from escrow
        let token_client = token::Client::new(&env, &bounty.payment_token);
        token_client.transfer(
            &env.current_contract_address(),
            developer,
            &bounty.payment_amount,
        );
        
        bounty.status = BountyStatus::Completed;
        env.storage().instance().set(&DataKey::Bounty(bounty_id), &bounty);
        
        // Update developer stats
        let mut dev_profile: DeveloperProfile = env.storage()
            .instance()
            .get(&DataKey::Developer(developer.clone()))
            .expect("Developer not found");
        
        dev_profile.completed_bounties += 1;
        env.storage().instance().set(&DataKey::Developer(developer.clone()), &dev_profile);
    }
    
    /// Dispute a bounty (can be called by company or developer)
    pub fn dispute_bounty(env: Env, bounty_id: u64, caller: Address) {
        caller.require_auth();
        
        let mut bounty: Bounty = env.storage()
            .instance()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");
        
        // Verify caller is company or assigned developer
        let is_authorized = bounty.company == caller || 
            bounty.assigned_developer.as_ref() == Some(&caller);
        
        assert!(is_authorized, "Not authorized");
        
        bounty.status = BountyStatus::Disputed;
        env.storage().instance().set(&DataKey::Bounty(bounty_id), &bounty);
    }
    
    /// Cancel bounty and refund (only if not assigned)
    pub fn cancel_bounty(env: Env, bounty_id: u64, company: Address) {
        company.require_auth();
        
        let mut bounty: Bounty = env.storage()
            .instance()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");
        
        assert!(bounty.company == company, "Not authorized");
        
        assert!(
            bounty.status == BountyStatus::Open,
            "Cannot cancel bounty in this state"
        );
        
        // Refund escrow
        let token_client = token::Client::new(&env, &bounty.payment_token);
        token_client.transfer(
            &env.current_contract_address(),
            &company,
            &bounty.payment_amount,
        );
        
        bounty.status = BountyStatus::Cancelled;
        env.storage().instance().set(&DataKey::Bounty(bounty_id), &bounty);
    }
    
    /// Get bounty details
    pub fn get_bounty(env: Env, bounty_id: u64) -> Option<Bounty> {
        env.storage().instance().get(&DataKey::Bounty(bounty_id))
    }
    
    /// Get company's bounties
    pub fn get_company_bounties(env: Env, company: Address) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::CompanyBounties(company))
            .unwrap_or(Vec::new(&env))
    }
    
    /// Get developer's bounties
    pub fn get_developer_bounties(env: Env, developer: Address) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::DeveloperBounties(developer))
            .unwrap_or(Vec::new(&env))
    }
    
    /// Rate developer (called by company after completion)
    pub fn rate_developer(
        env: Env,
        bounty_id: u64,
        company: Address,
        rating: u32, // 0-100
    ) {
        company.require_auth();
        
        let bounty: Bounty = env.storage()
            .instance()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");
        
        assert!(bounty.company == company, "Not authorized");
        
        assert!(
            bounty.status == BountyStatus::Completed,
            "Bounty not completed"
        );
        
        let developer = bounty.assigned_developer.expect("No developer assigned");
        let mut dev_profile: DeveloperProfile = env.storage()
            .instance()
            .get(&DataKey::Developer(developer.clone()))
            .expect("Developer not found");
        
        // Simple average rating calculation
        let total_ratings = dev_profile.completed_bounties as u32;
        if total_ratings > 0 {
            let current_total = dev_profile.rating * (total_ratings - 1);
            dev_profile.rating = (current_total + rating) / total_ratings;
        } else {
            dev_profile.rating = rating;
        }
        
        env.storage().instance().set(&DataKey::Developer(developer), &dev_profile);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bounty_status_comparison() {
        let status1 = BountyStatus::Open;
        let status2 = BountyStatus::Open;
        let status3 = BountyStatus::Assigned;
        
        assert!(status1 == status2);
        assert!(status1 != status3);
    }
}