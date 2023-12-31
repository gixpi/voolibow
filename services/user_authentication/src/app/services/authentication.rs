use crate::app::models::token::Token;
use crate::app::models::user::User;
use crate::app::models;
use crate::app::types::error::Error;
use crate::authentication_proto::{SignupRequest,OptionalResponse, TokenInfo,VerificationRequest, SigninRequest};

use std::sync::Arc;
use bb8_redis::{RedisConnectionManager,redis::AsyncCommands};
use sqlx::{Postgres, Row};
use super::common;

pub async fn signup(pg_db_pool:&sqlx::Pool<Postgres>, rd_db_pool:&bb8::Pool<RedisConnectionManager>, data:SignupRequest)->Result<OptionalResponse,Error>{
    let rgx = regex::Regex::new(r#"^(\+98|0)?9\d{9}$"#).map_err(|e|return Error::InternalError(e.to_string()))?;
    if !rgx.is_match(&data.phone){
        return Err(Error::ServiceError("the format of phone number is incorrect #111".to_owned()))
    };

    let pn_result_rd = common::phone_number_exists_rd(rd_db_pool, &data.phone).await;
    match pn_result_rd{
        Ok(_)=>return Err(Error::ServiceError("you've already made a proccess line. try later #111".to_owned())),
        Err(e)=>{
            match e{
                Error::NotFoundError(_) => {},
                Error::InternalError(e) => return Err(Error::InternalError(e)),
                _=>return Err(Error::InternalError("rgx #111".to_owned())),
            }
        }
    }
    let phone_number = Arc::new(data.phone);

    common::user_exists_by_phone_pg(pg_db_pool, &phone_number.clone()).await?;
    let value = serde_json::to_string(&models::user::User::new(&phone_number.clone()))
    .map_err(|_|return Error::InternalError("try later #711".to_owned()))?;

    let mut rd_db_pool =  rd_db_pool.get()
    .await
    .map_err(|_|return Error::InternalError("try later #555".to_owned()))?;

    let code = idgen::numeric_code_i16(1245, 9864);

    // pkg::SMS::new_verification_message(code,"_".to_owned(), vec![phone_number.as_ref().to_owned()])
    // .send_sms().await?;

    let _:() = rd_db_pool.set_ex::<String,String,()>(phone_number.as_ref().to_owned(), value, 120)
    .await
    .map_err(|_|return Error::InternalError("try later #712".to_owned()))?;

    let _:() = rd_db_pool.set_ex::<String,String,()>(code.to_string(), phone_number.as_ref().to_owned(), 120)
    .await
    .map_err(|_|return Error::InternalError("try later #712".to_owned()))?;

    Ok(OptionalResponse {  msg: None,code:Some(code.to_string()) })
}

pub async fn verify(pg_db_pool:&sqlx::Pool<Postgres>,rd_db_pool:&bb8::Pool<RedisConnectionManager>,data:VerificationRequest,expiry:i32)->Result<TokenInfo,Error>{
    let mut rd_db_pool =  rd_db_pool.get()
    .await
    .map_err(|_|return Error::InternalError("try later #555".to_owned()))?;

    match data.verification_method{
        0=>{
            let phone_number:Option<String> =  rd_db_pool.get_del(data.code.clone()).await.map_err(|_|return Error::InternalError("try later #712".to_owned()))?;
        if phone_number.is_none(){
            return Err(Error::NotFoundError("code not found #404".to_owned()))
        }
        let phone_number = phone_number.unwrap();
        let user_json_data:String =  rd_db_pool.get_del(phone_number.clone()).await.map_err(|_|return Error::InternalError("try later #712".to_owned()))?;

        let user = serde_json::from_str::<User>(user_json_data.as_str())
        .map_err(|_|return Error::InternalError("try later #711".to_owned()))?;
        
        sqlx::query("INSERT INTO users (user_id,phone_number,user_role,user_status,created_at) VALUES ($1,$2,$3,$4,$5)")
        .bind(user.user_id)
        .bind(user.phone_number)
        .bind(user.role.to_string())
        .bind(user.user_status.to_string())
        .bind(user.created_at)
        .execute(pg_db_pool)
        .await
        .map_err(|e|return Error::InternalError(e.to_string()))?;
        
        let token = Token::new(user.user_id, data.agent, data.ip,expiry);
        common::create_token(pg_db_pool, &token).await?;
        return Ok(TokenInfo { access_token: token.access_token, refresh_token: token.refresh_token, expiry: expiry})
        }
        1=>{
            let phone_number:Option<String> =  rd_db_pool.get_del(data.code.clone()).await.map_err(|_|return Error::InternalError("try later #712".to_owned()))?;
        if phone_number.is_none(){
            return Err(Error::NotFoundError("code not found #404".to_owned()))
        }
        let phone_number = phone_number.unwrap();
        let pn_result_pg = common::user_exists_by_phone_pg(pg_db_pool, &phone_number).await;
        match pn_result_pg{
            Ok(_)=>return Err(Error::NotFoundError("phone number not found #404".to_owned())),
            Err(e)=>{
                match e{
                    Error::ServiceError(_)=>{}
                    Error::InternalError(es)=>return Err(Error::InternalError(es)),
                    _=>return Err(Error::NotFoundError("phone number not found #404".to_owned()))
                }
            }
        }
        let user_id = common::get_user_id_by_phone_number_pg(pg_db_pool, &phone_number).await?;
        if user_id.is_none(){
            return Err(Error::NotFoundError("phone number not found #404".to_owned()))
        }
        let token = Token::new(user_id.unwrap(), data.agent, data.ip,expiry);
        common::create_token(pg_db_pool, &token).await?;
        return Ok(TokenInfo { access_token: token.access_token, refresh_token: token.refresh_token, expiry: expiry})
        }
        _=>{
            return Err(Error::ServiceError("unkown method #400".to_owned()))
        }
    } 
}

pub async fn signin(pg_db_pool:&sqlx::Pool<Postgres>,rd_db_pool:&bb8::Pool<RedisConnectionManager>,data:SigninRequest)->Result<OptionalResponse,Error>{
    let phone_number = Arc::new(data.phone);
    let pn_result_pg = common::user_exists_by_phone_pg(pg_db_pool, &phone_number.clone()).await;
    match pn_result_pg{
        Ok(_)=>return Err(Error::NotFoundError("phone number not found #404".to_owned())),
        Err(e)=>{
            match e{
                Error::ServiceError(_)=>{}
                Error::InternalError(es)=>return Err(Error::InternalError(es)),
                _=>return Err(Error::NotFoundError("phone number not found #404".to_owned()))
            }
        }
    }
    
    let row = sqlx::query("SELECT user_status FROM users WHERE phone_number = $1")
    .bind(phone_number.as_ref())
    .fetch_one(pg_db_pool)
    .await
    .map_err(|e|return Error::InternalError(e.to_string()))?;

    let user_status = models::user::Status::from(row.get::<String,_>("user_status"));
    if !user_status.validate_status(){
        return Err(Error::ServiceError(format!("your account stauts: {} #400",user_status.to_string())))
    }

    let mut rd_db_pool =  rd_db_pool.get()
    .await
    .map_err(|_|return Error::InternalError("try later #555".to_owned()))?;

    let code = idgen::numeric_code_i16(1245, 9864);

    // pkg::SMS::new_verification_message(code,"_".to_owned(), vec![phone_number.as_ref().to_owned()])
    // .send_sms().await?;
    
    let _:() = rd_db_pool.set_ex::<String,String,()>(code.to_string(), phone_number.as_ref().to_owned(), 120)
    .await
    .map_err(|_|return Error::InternalError("try later #712".to_owned()))?;
    
    Ok(OptionalResponse {  msg: None,code:Some(code.to_string()) })
} 