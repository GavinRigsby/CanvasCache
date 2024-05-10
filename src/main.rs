use reqwest::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION};
use serde::de::{Error, Visitor};
use tokio::io::BufWriter;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use lazy_static::lazy_static;
use filenamify::filenamify;
use regex::Regex;
use serde_json::Value;
use chrono::{DateTime, Utc};
use tokio::task;
use serde::{Deserialize, Serialize, Deserializer};
use std::sync::{Arc, Mutex};
use maplit::hashmap;

#[macro_use]
extern crate fstrings;

#[derive(Serialize, Deserialize, Debug)]
pub struct Course {
    pub id: i32,
    pub name: Option<String>,
    pub account_id: Option<i32>,
    pub default_view: Option<String>,
    pub course_code: Option<String>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Folder {
    pub id: i32,
    pub full_name: Option<String>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct File {
    pub id: i32,
    pub folder_id: i32,
    pub filename: Option<String>,
    pub url: Option<String>,
    pub modified_at: Option<String>,
    pub locked_for_user: bool,
    pub lock_explaination: Option<String>,
    pub display_name: Option<String>
}

impl File {
    // Parse modified_at string into DateTime<Utc>
    fn parsed_modified_at(&self) -> Option<DateTime<Utc>> {
        self.modified_at
            .as_ref()
            .and_then(|dt_str| DateTime::parse_from_rfc3339(dt_str).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }
}


impl Ord for File {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let self_modified_at = self.parsed_modified_at();
        let other_modified_at = other.parsed_modified_at();
        match (self_modified_at, other_modified_at) {
            (Some(self_dt), Some(other_dt)) => other_dt.cmp(&self_dt), // Reversed for newest files first
            (None, Some(_)) => std::cmp::Ordering::Less, // None is considered less than a Some value
            (Some(_), None) => std::cmp::Ordering::Greater, // Some value is considered greater than None
            (None, None) => std::cmp::Ordering::Equal, // Both are None, considered equal
        }
    }
}

impl PartialOrd for File {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}


impl PartialEq for File {
    fn eq(&self, other: &Self) -> bool {
        self.folder_id == other.folder_id && self.filename == other.filename
    }
}

impl Eq for File {}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Page {
    pub url: String,
    pub updated_at: Option<String>
}

impl Page {
    // Parse modified_at string into DateTime<Utc>
    fn parsed_modified_at(&self) -> Option<DateTime<Utc>> {
        self.updated_at
            .as_ref()
            .and_then(|dt_str| DateTime::parse_from_rfc3339(dt_str).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PageContent {
    pub body: Option<String>,
    pub locked_for_user: Option<bool>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Announcement {
    pub id: String,
    pub title: String,
    pub created_at: Option<String>,
    pub message: String
  }

impl Announcement {
    // Parse modified_at string into DateTime<Utc>
    fn parsed_modified_at(&self) -> Option<DateTime<Utc>> {
        self.created_at
            .as_ref()
            .and_then(|dt_str| DateTime::parse_from_rfc3339(dt_str).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Module {
    pub id: i32,
    pub name: String,
    pub position: i32,
    pub unlock_at: Option<String>,
    pub require_sequential_progress: bool,
    pub publish_final_grade: bool,
    pub prerequisite_module_ids: Vec<i32>,
    pub state: Option<String>,
    pub completed_at: Option<String>, // iso date string
    pub items_count: i32,
    pub items_url: String
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModuleItem  {
    pub id: i32,
    pub title: String,
    pub position: i32,
    pub indent: i32,
    #[serde(rename = "type")]
    pub item_type: String,
    pub module_id: i32,
    pub html_url: Option<String>,
    pub content_id: Option<i32>,
    pub url: Option<String>,
    pub external_url: Option<String>
  }

  #[derive(Serialize, Deserialize, Debug, Clone)]
  pub struct  User {
    pub id: i32,
    pub name: String
  }

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Assignment {
    pub id: i32,
    pub html_url: String,
    pub name: String,
    pub description: Option<String>,
    pub points_possible: f32,
    pub rubric: Option<Vec<Rubric>>,
    pub updated_at: Option<String>,
    pub is_quiz_assignment: bool,
    pub quiz_id: Option<i32>
}

impl Assignment {
    // Parse modified_at string into DateTime<Utc>
    fn parsed_modified_at(&self) -> Option<DateTime<Utc>> {
        self.updated_at
            .as_ref()
            .and_then(|dt_str| DateTime::parse_from_rfc3339(dt_str).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }
}


  #[derive(Serialize, Deserialize, Debug, Clone)]
  pub struct Rubric {
    pub id: String,
    pub points: f32,
    pub description: Option<String>,
    pub long_description: Option<String>,
    pub ratings: Vec<Rating>
  }

  #[derive(Serialize, Deserialize, Debug, Clone)]
  pub struct Rating {
    pub id: String,
    pub points: f32,
    pub description: Option<String>,
    pub long_description: Option<String>
  }
  


#[derive(Serialize, Debug, Clone)]
pub struct Submission {
    pub id: i32,
    pub score: Option<f32>,
    pub grade_matches_current_submission: bool,
    pub late: bool,
    pub missing: bool,
    pub submission_type: Option<String>,
    pub submission_history: Option<Vec<Submission>>,
    pub type_specific_info: AssignmentData
}

impl<'de> Deserialize<'de> for Submission {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Debug, Deserialize)]
        struct Mapping {
            pub id: i32,
            pub score: Option<f32>,
            pub grade_matches_current_submission: bool,
            pub late: bool,
            pub missing: bool,
            pub submission_type: Option<String>,
            pub submission_history: Option<Vec<Submission>>,
            pub submission_data: Option<Vec<QuizItem>>,
            pub attachments: Option<Vec<UploadAttachment>>
        }

        let Mapping {
            id, score, grade_matches_current_submission, late, missing,
            submission_type, submission_history, submission_data, attachments
        }  = Mapping::deserialize(deserializer)?;



        let a = submission_data;
        let b = attachments;
        let c = match submission_type.clone() {
            Some(str) => str,
            None => "NONE".to_string()
        };
        
        match c.as_str() {

            "online_quiz" => {
                let type_specific_info = AssignmentData::Quiz(QuizData { submission_data: a });
                Ok(
                    Submission { 
                        id, score, grade_matches_current_submission, late, missing, 
                        submission_type, submission_history, type_specific_info}
                )
            },

            "online_upload" => {
                let type_specific_info = AssignmentData::Upload(UploadData { attachments: b });
                Ok(
                    Submission { 
                        id, score, grade_matches_current_submission, late, missing,
                        submission_type, submission_history, type_specific_info}
                )
            }

            _ => {
                println!("[ERROR] Unknown submission type {}",c.as_str());
                Err(D::Error::custom("Submission Type Unknown"))
            },
        }
    }
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UploadData {
    pub attachments: Option<Vec<UploadAttachment>>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct QuizData {
    pub submission_data: Option<Vec<QuizItem>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AssignmentData {
    Quiz(QuizData),
    Upload(UploadData)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct QuizItem {
    pub correct: CorrectValue,
    pub points: f32, 
    pub question_id: i32,
    pub answer_id: Option<i32>,
    pub text: String,
    pub more_comments: Option<String>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct QuizQuestion {
    pub id: i32,
    pub question_name: String,
    pub question_type: String,
    pub question_text: String,
    //pub answers: Vec<QuizMultipleChoiceAnswer>
}

#[derive(Serialize, Debug, Clone)]
pub enum CorrectValue {
    Bool(bool),
    String(String),
}


impl<'de> Deserialize<'de> for CorrectValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct CorrectValueVisitor;

        impl<'de> Visitor<'de> for CorrectValueVisitor {
            type Value = CorrectValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a boolean or string value")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(CorrectValue::Bool(v))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(CorrectValue::String(v.to_string()))
            }
        }

        deserializer.deserialize_any(CorrectValueVisitor)
    }
}

  #[derive(Serialize, Deserialize, Debug, Clone)]
  pub struct SubmissionData {
    pub id: i32,
    pub correct: bool,
    pub points: f32,
    pub question_id: i32, 
    pub answer_id: i32, 
    pub text: String,
    pub more_comments: String
  }

  #[derive(Serialize, Deserialize, Debug, Clone)]
  pub struct UploadAttachment {
    pub id: i32, 
    pub display_name: String,
    pub filename: String,
    pub url: String,
    pub modified_at: Option<String>
  }

  impl UploadAttachment {
    // Parse modified_at string into DateTime<Utc>
    fn parsed_modified_at(&self) -> Option<DateTime<Utc>> {
        self.modified_at
            .as_ref()
            .and_then(|dt_str| DateTime::parse_from_rfc3339(dt_str).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }
}

lazy_static! {
    static ref FILE_PATHS: Mutex<HashMap<(i32, i32), String>> = {
        let mut map = HashMap::new();
        Mutex::new(map)
    };

    static ref PAGE_PATHS:Mutex<HashMap<(i32, i32), String>> = {
        let mut map = HashMap::new();
        Mutex::new(map)
    };

    static ref FOLDER_PATHS:Mutex<HashMap<(i32, i32), String>> = {
        let mut map = HashMap::new();
        Mutex::new(map)
    };
}

fn backup_file_paths(){
    let paths = FILE_PATHS.lock().unwrap();

    let _ = create_directory("./", ".canvascache");

    let file = std::fs::File::create("./.canvascache/files").expect("Failed to create file");
    let mut writer = std::io::BufWriter::new(file);

    for ((id1, id2), path) in paths.iter() {
        writeln!(&mut writer, "ID1: {}, ID2: {}, Path: {}", id1, id2, path).expect("Failed to write to file");
    }
}

fn restore_file_paths(){

    if !Path::new("./.canvascache/files").exists() {
        return;
    }

    let file = std::fs::File::open("./.canvascache/files").expect("Failed to open file");
    let reader = std::io::BufReader::new(file);

    // Iterate over each line in the file
    for line in reader.lines() {
        if let Ok(line) = line {
            // Parse the line into ID1, ID2, and pathz
            let parts: Vec<&str> = line.split(", ").collect();
            if parts.len() == 3 {
                if let (Ok(id1), Ok(id2)) = (parts[0].parse::<i32>(), parts[1].parse::<i32>()) {
                    let path = parts[2].to_string();
                    // Insert the ID and path into the FILE_PATHS hashmap
                    add_file_path(id1, id2, path.as_str());
                }
            }
        }
    }
}

fn get_file_path(course_id: i32, file_identifier: i32) -> Option<String> {
    // Access the global HashMap, lock it, and return the value associated with the provided tuple key
    FILE_PATHS
        .lock()
        .unwrap()
        .get(&(course_id, file_identifier))
        .cloned()
}

fn add_file_path(course_id: i32, file_identifier: i32, path: &str) {
    // Access the global HashMap, lock it, and insert a new key-value pair
    FILE_PATHS
        .lock()
        .unwrap()
        .insert((course_id, file_identifier), path.to_string());
}

fn get_folder_path(course_id: i32, folder_identifier: i32) -> Option<String> {
    // Access the global HashMap, lock it, and return the value associated with the provided tuple key
    FILE_PATHS
        .lock()
        .unwrap()
        .get(&(course_id, folder_identifier))
        .cloned()
}

fn add_folder_path(course_id: i32, folder_identifier: i32, path: &str) {
    // Access the global HashMap, lock it, and insert a new key-value pair
    FILE_PATHS
        .lock()
        .unwrap()
        .insert((course_id, folder_identifier), path.to_string());
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //let mut opt = Opt::from_args();
    
    //opt.append_api_suffix();

    std::env::set_var("RUST_BACKTRACE", "FULL");

    restore_file_paths();

    
    let token = "YOUR_CANVAS_TOKEN";
    let url = "YOUR_CANVAS_API";
    let dest = "../CanvasDump/";

    
    let course_url = f!("{}{}", url, "/courses");
    let catalog = request_json::<Course>(&course_url.as_str(), &token, None).await?;

    fs::create_dir_all(dest)?;

    for course in catalog.get(){

        if course.name.is_none() {
            //println!("Course with ID {} has no name.", course.id);
            continue;
        }
        
        let course_name = course.name.unwrap();
        let course_id = course.id;
        let course_dir = Path::new(dest).join(sanitize_file_name(&course_name));
        fs::create_dir_all(&course_dir)?;

        println!("============= Download Course {} ==================", &course_name);

        let course_dir_str = &course_dir.to_str().unwrap();
        
        let _ = download_files(url, course_id, token, course_dir_str ).await;
        let _ = download_pages(url, course_id, token, course_dir_str).await;
        let _ = download_announcements(url, course_id, token, course_dir_str).await;
        let _ = download_assignments(url, course_id, token, course_dir_str).await;
        let _ = download_modules(url, course_id, token, course_dir_str).await;
    }
    
    backup_file_paths();
    //download(opt).await?;
    Ok(())
}


fn create_directory(base_path: &str, directory_name: &str) -> Result<String, ()>{
    let new_dir = f!("{}/{}/", base_path, directory_name);
    match fs::create_dir_all(&new_dir) {
        Ok(_) => {
            ()
        },
        Err(_) => {
            println!("[ERROR] Directory Not Created {}", new_dir);
        }
    };
    Ok(new_dir)
}

async fn download_assignments(url: &str, course_id: i32, token: &str, course_dir: &str, ) -> Result<(), String> {
    let course_id_url = f!("{}/courses/{}", url, course_id);
    let course_url = course_id_url.as_str();

    let assignment_url = f!("{}{}", course_url, "/assignments");

    println!("{}",assignment_url);
    let assignments = match request_json::<Assignment>(&assignment_url.as_str(), token, None).await{
        Ok(assignments) => {
            assignments.get()
        },
        Err(_) => {
            Vec::<Assignment>::new()
        }
    };

    if assignments.is_empty(){
        println!("[INFO] No Assignments Found");
        return Ok(());
    }

    let user_url = f!("{}/users/self", url);
    let users = match request_json::<User>(&user_url.as_str(), token, None).await{
        Ok(users) => {
            users.get()
        },
        Err(_) => {
            Vec::<User>::new()
        }
    };

    let user = users.get(0).unwrap();

    for assignment in assignments {


        
        let quiz_id = match assignment.quiz_id {
            Some(id) => id,
            None => 0
        };

        let assignment_dir = f!("{}/assignments/{}", course_dir, sanitize_file_name(assignment.name.as_str()));
        match fs::create_dir_all(&assignment_dir) {
            Ok(_) => {
                ()
            },
            Err(_) => {
                println!("[ERROR] Directory Not Created {}", assignment_dir);
            }
        };

        // Write the description to a file
        let assignment_modified = assignment.parsed_modified_at().unwrap();
        let mut assignment_description = assignment.description.unwrap_or("".to_string());

        if let Some(rubric) = assignment.rubric {
            assignment_description += "\n";
            for item in rubric {
                let text = match item.long_description {
                    Some(long_desc) if !long_desc.is_empty() => long_desc,
                    _ => item.description.unwrap_or_default(),
                };

                assignment_description += f!("[{} Points] {}\n", item.points, text).as_str();

                for rating in item.ratings {

                    let text = match rating.long_description {
                        Some(long_desc) if !long_desc.is_empty() => long_desc,
                        _ => rating.description.unwrap_or_default(),
                    };

                    assignment_description += f!("{} ===> {}\n", text, rating.points).as_str();
                }

                assignment_description += "\n\n";
            }
        }

        let description_path = f!("{}/description.txt", assignment_dir);
        write_file_if_newer(&assignment_description, &description_path, assignment_modified).await.unwrap();
    
        let submission_url = f!("{}/{}/submissions/{}", assignment_url, assignment.id, user.id);
        let params = hashmap!{"include[]".to_string() => "submission_history".to_string()};
        let submissions = match request_json::<Submission>(&submission_url.as_str(), token, Some(params)).await{
            Ok(submissions) => {
                submissions.get()
            },
            Err(_) => {
                Vec::<Submission>::new()
            }
        };

        let submissions_dir = create_directory(&assignment_dir,"submissions", ).unwrap();

        let mut keep_going = true;

        for submission in submissions {

            let mut index = 1;
            
            for attempt in submission.submission_history.unwrap() {

                let attempt_dir = create_directory(&submissions_dir, f!("Attempt_{}", index).as_str()).unwrap();
                
                if !keep_going { break }

                index += 1;

                if let AssignmentData::Quiz(quiz) = attempt.type_specific_info {
                    assert_ne!(quiz_id, 0, "Quiz ID Should never be 0 (incorrectly mapped assignment)");
                    
                    if let Some(quiz_items) = quiz.submission_data {
                        
                        //https://canvas.ewu.edu/api/v1/quiz_submissions/15880916/questions?include[]=quiz_question
                        // Since we know its a quiz and has data lets try and get the questions
                        let question_url = f!("{}/quiz_submissions/{}/questions", url, quiz_id);
                        let params = hashmap!{"include[]".to_string() => "quiz_question".to_string() };

                        let questions = match request_json::<QuizQuestion>(question_url.as_str(), token, Some(params)).await {
                            Ok(questions) => {
                                questions.get()
                            },
                            Err(_) => {
                                Vec::<QuizQuestion>::new()
                            }
                        };

                        if questions.is_empty() {
                            println!("Could not find questions for quiz {}", quiz_id);
                            keep_going = false;
                            break;
                        }
                        // After this questions can be assumed as existing

                        println!("FOUND QUESTIONS {}", question_url);
                        for item in quiz_items {
                            
                            let question_id = item.question_id;
                            
                            let mut i: isize = -1;
                            if let Some(q) = questions.iter().position(|q| q.id == question_id) {
                                i = q as isize;
                            }

                            if i == -1 {
                                println!("No question matching {} found", question_id);
                                continue
                            }

                            let question_item = questions.get(i as usize).unwrap();
                            

                            if let Some(answer_id) = item.answer_id {
                                // Multiple choice use answer ID to get selected answer
                                
                            }
                            else {
                                // Short Answer get the answer from text
                                let answer = item.text;
                            }
                        }
                    }else{
                        println!("No items found for Quiz {}", quiz_id)
                    }

                }
                else if let AssignmentData::Upload(upload) = attempt.type_specific_info {
                    let attachment_dir =  create_directory(attempt_dir.as_str(),  "attachments").unwrap();

                    if let Some(attachments) = upload.attachments{
                        for attachment in attachments{
                            let attachment_path = f!("{}/{}", attachment_dir, sanitize_file_name(attachment.filename.as_str()));
                            let attachment_url = attachment.url.as_str();
                            let course_id = match extract_number_after(attachment_url, "/course/"){
                                Some(s) => s.parse::<i32>().unwrap(),
                                None => 0
                            };
                            let file_id = extract_number_after(attachment_url, "/files/").unwrap().parse::<i32>().unwrap();
                           
                            let path = get_file_path(course_id, file_id);

                            if let Some(file) = path {
                                create_symlink(file.as_str(), attachment_path.as_str());
                            }else {
                                let _ = download_from_url(attachment.url.as_str(), token,  &attachment_path, attachment.parsed_modified_at().unwrap()).await;
                                add_file_path(course_id, file_id, &attachment_path);
                            }
                        }
                    }
                    else {
                        println!("No Attachments found for assignment!")
                    }
                    
                }
                else {
                    println!("Submission type not mapped");
                }
            }
        }
    }
    Ok(())
}

async fn download_files(url: &str, course_id: i32, token: &str, course_dir: &str, ) -> Result<(), String>{

    let course_id_url = f!("{}/courses/{}", url, course_id);
    let course_url = course_id_url.as_str();

    let folder_url = f!("{}{}", course_url, "/folders");
    
    let folders = match request_json::<Folder>(folder_url.as_str(), token, None).await {
        Ok(folders) => {
            folders.get()
        },
        Err(_) => {
            Vec::<Folder>::new()
        }
    };

    // Create all folders
    for folder in &folders {
        let folder_name = folder.full_name.clone().unwrap();
            //println!("Parent Folder {} for file {}", folder_name, file_name);
        let dest_folder = f!("{}/files/{}", course_dir, sanitize_folder_name(&folder_name));

        let dir_create = fs::create_dir_all(&dest_folder);

        match dir_create {
            Ok(_) => {
                add_folder_path(course_id, folder.id, &dest_folder)
            },
            Err(_) => {
                println!("[ERROR] Directory Not Created {}", dest_folder);
            }
        }
    }
    
    let file_url = f!("{}{}", course_url, "/files");
    let file_request = request_json::<File>(file_url.as_str(), token, None).await;
    
    let mut files = match file_request {
        Ok(files) => {
            files.get()
        },
        Err(_) => {
            Vec::<File>::new()
        }
    };

    if files.is_empty(){
        println!("[INFO] No Files Found");
        return Ok(());
    }

    // Sort files newest to oldest
    files.sort();

    // Remove duplicates based on folder_id and filename.
    let mut unique_files = Vec::new();
    let mut last_file: Option<File> = None;
    for file in files.into_iter() {
        if last_file.is_none() || last_file.as_ref().unwrap() != &file {
            unique_files.push(file.clone());
            last_file = Some(file);
        }
    }

    // Replace the current files vector with the unique and sorted files.
    files = unique_files;

    let barrier = Arc::new(tokio::sync::Barrier::new(files.len() + 1));

    for file in files {
        let file_name = sanitize_file_name(file.display_name.clone().unwrap().as_str());
        //println!("Found File {0}", file_name);

        if file.locked_for_user {
            println!("[SKIP] File {} skipped. Reason: {}", file_name, file.lock_explaination.unwrap());
            continue;
        }

        let parent_folder: Vec<&Folder> = folders
        .iter()
        .filter(|&x| x.id == file.folder_id).collect();

        if parent_folder.is_empty() {
            println!("[ERROR] Parent folder not found for {}", file_name);
        }

        let folder_name = parent_folder.get(0).unwrap().full_name.clone().unwrap();
            //println!("Parent Folder {} for file {}", folder_name, file_name);
        let dest_folder = f!("{}/files/{}", course_dir, sanitize_folder_name(&folder_name));

        let dest_path = f!("{}/{}", &dest_folder, file_name);

        add_file_path(course_id, file.id, &dest_path);

        let modified_time = file.parsed_modified_at().unwrap();

        let file_url = file.url.unwrap();
        let url_token = token.to_string();
        let barrier = barrier.clone();

        task::spawn(async move {
            download_from_url(&file_url, &url_token, &dest_path, modified_time).await;
            barrier.wait().await;
        });
    }

    barrier.wait().await;

    Ok(())
}

async fn download_pages(url: &str, course_id: i32, token: &str, course_dir: &str) -> Result<HashMap<String, String>, String> {
    
    let mut map :HashMap<String, String> = HashMap::new();    

    let course_id_url = f!("{}/courses/{}", url, course_id);
    let course_url = course_id_url.as_str();
    
    let page_url = f!("{}{}", course_url, "/pages");
    let page_request = request_json::<Page>(&page_url.as_str(), token, None).await;

    let pages = match page_request {
        Ok(pages) => {
            pages.get()
        },
        Err(_) => {
            Vec::<Page>::new()
        }
    };

    if pages.is_empty() {
        println!("[INFO] No Pages Found");
        return Ok(map)
    }

    let page_dir = f!("{}{}", course_dir, "/pages");
    let dir_create = fs::create_dir_all(&page_dir);

    match dir_create {
        Ok(_) => {
            ()
        },
        Err(_) => {
            println!("[ERROR] Directory Not Created {}", page_dir);
        }
    }

    for page in pages {
        
        
        let p_url = &page.url;
        
        let dest_path = f!("{}/{}.html", page_dir, filenamify(p_url).replace(" ", "_")); 
        let specific_page_url = f!("{}{}{}", course_url, "/pages/", p_url);
        let modified_datetime = page.parsed_modified_at().unwrap();

        
        let metadata = fs::metadata(&dest_path);
        let file_exists = metadata.is_ok();
        let current_modified_time = metadata
            .ok()
            .map(|meta| meta.modified().ok())
            .flatten()
            .unwrap_or(SystemTime::UNIX_EPOCH);

        if file_exists && current_modified_time >= modified_datetime.into() {
            println!("[SKIP] {}", dest_path);
            map.insert(p_url.to_string(), dest_path);
            continue;
        }

        let page_request = request_json::<PageContent>(&specific_page_url.as_str(), token, None).await;

        let page_contents = match page_request {
            Ok(pages) => {
                pages.get()
            },
            Err(e) => {
                println!("{}",e);
                Vec::<PageContent>::new()
            }
        };


        let mut content: String = "".to_string();
        for page_content in page_contents {

            let body_tag = page_content.body.as_ref();
            content += body_tag.unwrap();
        }

        map.insert(p_url.to_string(), dest_path.clone());

        if content.is_empty() {
            println!("[INFO] No body for page {}", p_url);
            continue;
        }

        write_file_if_newer(&content, &dest_path, modified_datetime).await.unwrap();
    }
    Ok(map)
}

async fn download_announcements(api_url: &str, course_id: i32, token: &str, course_dir: &str) -> Result<(), String> {
    let announcement_url = f!("{}{}", api_url, "/announcements");

    let params = hashmap!{"context_codes[]".to_string() => f!("course_{}",course_id)};
    let announcement_request = request_json::<Announcement>(&announcement_url.as_str(), token, Some(params)).await;

    let announcements = match announcement_request {
        Ok(announcements) => {
            announcements.get()
        },
        Err(_) => {
            Vec::<Announcement>::new()
        }
    };

    if announcements.is_empty() {
        println!("[INFO] No Announcements Found");
        return Ok(());
    }

    let announcement_dir = f!("{}{}", course_dir, "/announcments");
    let dir_create = fs::create_dir_all(&announcement_dir);

    match dir_create {
        Ok(_) => {
            ()
        },
        Err(_) => {
            println!("[ERROR] Directory Not Created {}", announcement_dir);
        }
    }


    for annoucement in announcements {
        let p_url = f!("{}_{}", &annoucement.title, annoucement.id);

        let dest_path = f!("{}/{}.html", announcement_dir, filenamify(p_url).replace(" ", "_"));
        let modified_datetime = annoucement.parsed_modified_at().unwrap();

        let message = annoucement.message;

        write_file_if_newer(&message, &dest_path, modified_datetime).await.unwrap();
    }
    Ok(())
}

fn extract_number_after<'a>(url: &'a str, pattern: &str) -> Option<&'a str> {
    if let Some(index) = url.find(pattern) {
        let start_index = index + pattern.len();
        if let Some(end_index) = url[start_index..].find('/') {
            Some(&url[start_index..start_index + end_index])
        } else {
            Some(&url[start_index..])
        }
    } else {
        None
    }
}

// Cross platform symlink function
pub fn create_symlink(target: &str, link_path: &str) {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link_path)
    }
    #[cfg(windows)]
    {

        let abs_t = std::fs::canonicalize(target).unwrap();
        // Convert to string (remove \\?\ from path to work with ShellLink)
        let abs_target = abs_t.to_string_lossy().to_string().replace("\\\\?\\", "");
        
        println!("Absolute path to target file {}", abs_target);

        let mut path = String::from(link_path); // Replace with your path

        // Check if the path ends with ".lnk"
        if !path.ends_with(".lnk") {
            // If not, append ".lnk" to the path
            path.push_str(".lnk");
        }

        let link = mslnk::ShellLink::new(abs_target).unwrap();

        let _ = link.create_lnk(path);

    }
    #[cfg(not(any(unix, windows)))]
    {
        // Handle other platforms here
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Symbolic link creation is not supported on this platform",
        ))
    }
}

async fn download_linked_file(file_url: String, token: String, course_id: i32, fallback_path: &str){
    let module_request = request_json::<File>(&file_url, &token, None).await;
    
    let file = match module_request {
        Ok(file) => {
            file.get()
        },
        Err(_) => {
            Vec::<File>::new()
        }
    };

    if file.len() > 1 {
        println!("[ERROR] More than 1 file found. Expected 1");
        return
    }

    if file.is_empty() {
        println!("[ERROR] No file found.");
        return
    }

    let target_file = file.get(0).unwrap();

    let folder_id = target_file.folder_id;

    let folder_path = get_folder_path(course_id, folder_id);

    let display_name = sanitize_file_name(target_file.display_name.clone().unwrap().as_str());
    let destination_path = if folder_path.is_some() {
        f!("{}/{}", folder_path.unwrap(), display_name)
    }else{
        f!("{}/{}", fallback_path, display_name)
    };

    let url = target_file.url.clone().unwrap();
    let url_token = token.clone();
    let path = destination_path.clone();
    let modified = target_file.parsed_modified_at().unwrap();

    let _ = download_from_url(&url, &url_token, &destination_path, modified).await;

    add_file_path(course_id, target_file.id, &path)
    // Can't find folder path
}

async fn download_modules(url: &str, course_id: i32, token: &str, course_dir: &str) -> Result<(), String> {

    let course_id_url = f!("{}/courses/{}", url, course_id);
    let course_url = course_id_url.as_str();

    let module_url = f!("{}/modules", course_url);

    let module_request = request_json::<Module>(module_url.as_str(), token, None).await;
    
    let modules = match module_request {
        Ok(modules) => {
            modules.get()
        },
        Err(_) => {
            Vec::<Module>::new()
        }
    };

    if modules.is_empty(){
        println!("[INFO] No Modules Found");
        return Ok(());
    }

    for module in modules {
        let safe_mod_name = sanitize_folder_name(&module.name);
        let module_dir = f!("{}/modules/{}", course_dir, safe_mod_name);
        let dir_create = fs::create_dir_all( &module_dir);

        match dir_create {
            Ok(_) => {
                ()
            },
            Err(_) => {
                println!("[ERROR] Directory Not Created {}", module_dir);
            }
        }

        let submodules_url = module.items_url;

        let submodules_request = request_json::<ModuleItem>(submodules_url.as_str(), token, None).await;
    
        let submodules = match submodules_request {
            Ok(submodules) => {
                submodules.get()
            },
            Err(_) => {
                Vec::<ModuleItem>::new()
            }
        };

        for submodule in submodules {

            let sanitized_name = sanitize_file_name(&submodule.title);
            let submodule_file_path = f!("{}/{}", module_dir, sanitized_name);

            if submodule.external_url.is_some() {
                println!("EXTERNAL SUBMODULE URL FOUND: {}", submodule.external_url.unwrap());
            }

            if submodule.url.is_none() {
                println!("NO SUBMODULE URL");
                continue;
            }

            let submodule_url = submodule.url.unwrap();

            if submodule_url.contains("/assignments/") {
                println!("ASSIGNMENT LINKED {}", submodule_url);
            } 
            else if submodule_url.contains("/discussion_topics/") {
                println!("DISCUSSION LINKED {}", submodule_url);
            } 
            else if submodule_url.contains("/files/") {
                println!("FILE LINKED {}", submodule_url);

                let file_id = extract_number_after(&submodule_url, "/files/");
                if file_id.is_none(){
                    println!("[ERROR] Could not extract file ID");
                }
                let str_file_id = file_id.unwrap();
                
                let file_identifier = match str_file_id.parse::<i32>() {
                    Ok(num) => {
                        num
                    }
                    Err (_) => {
                        println!("[ERROR] Could not parse File ID from {}", str_file_id);
                        0
                    }
                };

                if file_identifier != 0 {
                    
                    let file_path = get_file_path(course_id, file_identifier);

                    // File already found
                    if file_path.is_some() {
                        let _ = create_symlink(&file_path.unwrap(), &submodule_file_path);
                        println!("[WRITE] Created SYMLINK {}", submodule_file_path);
                        continue;
                    }
                }

                // File not found or parsing not working
                println!("[INFO] File not in dictionary. {}", submodule_file_path);
                let fallback = f!("{}/files", course_dir);
                let _ = download_linked_file(submodule_url, token.to_string(), course_id, &fallback).await;
                

                let file_path = get_file_path(course_id, file_identifier);

                let _ = create_symlink(&file_path.unwrap(), &submodule_file_path);
                println!("[WRITE] Created SYMLINK {}", submodule_file_path);
                continue;
            }
            else if submodule_url.contains("/quizzes/") {
                println!("QUIZ LINKED {}", submodule_url);
            }
            else if submodule_url.contains("/pages/") {
                println!("PAGE LINKED {}", submodule_url);
            }
            else {
                println!("[INFO] unknown type for {}", submodule_url);
            }

        }
    }

    Ok(())
}


#[derive(Debug)]
enum DownloadError {
    IoError(std::io::Error),
    HttpError(reqwest::Error),
    // Add more error types as needed
}

impl From<std::io::Error> for DownloadError {
    fn from(err: std::io::Error) -> Self {
        DownloadError::IoError(err)
    }
}

impl From<reqwest::Error> for DownloadError {
    fn from(err: reqwest::Error) -> Self {
        DownloadError::HttpError(err)
    }
}

enum DownloadResult {
    Success,
    Error(DownloadError),
}


#[derive(Debug)]
enum DataWrapper<T> {
    Single(T),
    Multiple(Vec<T>),
}

impl<T> DataWrapper<T> {
    pub fn get(self) -> Vec<T> {
        match self {
            DataWrapper::Multiple(mult) => mult,
            DataWrapper::Single(single) => vec![single],
        }
    }
}

impl<'de, T> Deserialize<'de> for DataWrapper<T>
where T: serde::de::DeserializeOwned,
{
    fn deserialize<D >(deserializer: D) -> Result<DataWrapper<T>, D::Error>
    where D: Deserializer<'de>,
    {
        let val: Value = Deserialize::deserialize(deserializer)?;

        match val {
            Value::Array(items) => {
                let vec: Vec<T> = items
                    .into_iter()
                    .map(|v| serde_json::from_value(v).map_err(serde::de::Error::custom))
                    .collect::<Result<Vec<T>, _>>()?;
                Ok(DataWrapper::Multiple(vec))
            }
            Value::Object(_) => {
                let single: T = serde_json::from_value(val).map_err(serde::de::Error::custom)?;
                Ok(DataWrapper::Single(single))
            }
            _ => Err(serde::de::Error::custom("Expected object or array")),
        }
    }
}


async fn download_from_url(
    url: &str,
    token: &str,
    destination_path: &str,
    modified_datetime: DateTime<Utc>,
) {
    match wrap_download(url, token, destination_path, modified_datetime).await{
        DownloadResult::Success => (),
        DownloadResult::Error(error) => {
            match error {
                DownloadError::IoError(e) => eprintln!("Error occured when downloading {}\n[ERROR] {}", url, e),
                DownloadError::HttpError(e) => eprintln!("Error occured when downloading {}\n[ERROR] {}", url, e),
            }
        }
    }
}

fn convert_datetime_to_systemtime(datetime: DateTime<Utc>) -> SystemTime {
    let timestamp = datetime.timestamp();
    UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64)
}

async fn wrap_download(
    url: &str,
    token: &str,
    destination_path: &str,
    modified_datetime: DateTime<Utc>) -> DownloadResult {
        match download_from_url_if_newer(url, token, destination_path, modified_datetime).await {
            Ok(_) => DownloadResult::Success,
            Err(e) => DownloadResult::Error(e.into())
        }
    }


async fn write_file_if_newer(
    content: &str,
    destination_path: &str,
    modified_datetime: DateTime<Utc>
) -> Result<(), ()> {

    // If not newer then skip
    if file_is_older(destination_path, modified_datetime).unwrap() {
        println!("[SKIP] {}", destination_path);
        return Ok(())
    }

    // Create Directory
    let parent_dir = Path::new(destination_path).parent().unwrap();
    tokio::fs::create_dir_all(parent_dir).await.map_err(|e| "Error Creating Directory").unwrap();

    // Write content to file
    let write_out = fs::write(&destination_path, content);
    match write_out {
        Ok(()) => (),
        Err(e) => eprintln!("Error Writing to File {}", e)
    };
    let open_file = std::fs::File::options().write(true).open(&destination_path).map_err(|_| "Error opening file").unwrap();
    
    // Set the modify time on the file
    let times = std::fs::FileTimes::new()
    .set_modified(convert_datetime_to_systemtime(modified_datetime));
    let set_times = open_file.set_times(times);
    match set_times {
        Ok(_) => (),
        Err(e) => eprintln!("Error Opening File {}", e)
    };
    println!("[WRITE] {}", destination_path);

    Ok(())
}

fn file_is_older(destination_path: &str, modified_datetime: DateTime<Utc>) -> Result<bool, ()> {
    let metadata = fs::metadata(destination_path);
    let file_exists = metadata.is_ok();
    let current_modified_time = metadata
        .ok()
        .map(|meta| meta.modified().ok())
        .flatten()
        .unwrap_or(SystemTime::UNIX_EPOCH);

    Ok(file_exists && current_modified_time >= modified_datetime.into())
}

async fn download_from_url_if_newer(
    url: &str,
    token: &str,
    destination_path: &str,
    modified_datetime: DateTime<Utc>
) -> Result<(), DownloadError> {

    if file_is_older(destination_path, modified_datetime).unwrap() {
        println!("[SKIP] {}", destination_path);
        return Ok(())
    }

    let client = Client::new();
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    // Create parent directories if they don't exist
    let parent_dir = Path::new(destination_path).parent().unwrap();
    tokio::fs::create_dir_all(parent_dir).await?;


    // Open destination file for writing
    let mut dest_file = BufWriter::new(tokio::fs::File::create(destination_path).await?);
    // Stream response body to destination file using tokio::io::copy
    tokio::io::copy(&mut response.bytes().await?.as_ref(), &mut dest_file).await?;
    
    //Change the modification time
    let dest = std::fs::File::options().write(true).open(destination_path)?;
    let times = std::fs::FileTimes::new()
        .set_modified(convert_datetime_to_systemtime(modified_datetime));
    dest.set_times(times)?;
    
    println!("[WRITE] {}", destination_path);
    Ok(())
}

fn sanitize_folder_name(folder_name: &str) -> String {
    let re = Regex::new(r"[^\w/]+").unwrap();
    re.replace_all(folder_name, "_").to_string()
}

fn sanitize_file_name(file_name: &str) -> String{
    filenamify(&file_name).replace(" ", "_").replace(",","")
}

fn extract_request_error(json_str: &str) -> Option<String> {
    // Parse the JSON string into a `serde_json::Value`.
    let v: Value = serde_json::from_str(json_str).unwrap();

    // Extract the `message` field from the JSON.
    if let Some(errors) = v.get("errors") {
        if let Some(first_error) = errors.get(0) {
            if let Some(message) = first_error.get("message") {
                if let Some(message_str) = message.as_str() {
                    return Some(message_str.to_string());
                }
            }
        }
    }

    // If the `message` field is not found, return None.
    None
}


async fn request_json<R: serde::de::DeserializeOwned >(url : &str, token: &str, query: Option<HashMap<String, String>>) -> Result<DataWrapper<R>, String> {
    
    let mut query_map = HashMap::new();

    if let Some(params) = query{
        query_map.extend(params);
    }
    
    query_map.insert("per_page".to_string(), "999999".to_string());

    let client = Client::new();
    let response = client
        .get(url)
        .header(AUTHORIZATION, f!("Bearer {}", token))
        .header(ACCEPT, "application/json")
        .query(&query_map)
        .send()
        .await
        .unwrap();

    match response.status() {
        reqwest::StatusCode::OK => {

            let data  = match response.text().await {
                Ok(data) => data,
                Err(e) => { 
                    let error = f!("{}",e);
                    println!("{}", error);
                    error
                }
            };

            if data.is_empty() {
                let json_err = f!("No Response Text found for {}", url);
                return Err(json_err);
            }


            let obj = serde_json::from_str(data.as_str()).map_err(|e| e.to_string());

            if let Err(e) = obj {
                println!("{}",e);
                println!("{}", url);
                return Err(e);
            }else{
                let o :DataWrapper<R> = obj.unwrap();
                return Ok(o);
            }
        },

        reqwest::StatusCode::UNAUTHORIZED => {
            let data = response.text().await.unwrap();
            println!("Unauthorized Access: {:?}", data);
            Err(data)
        },

        _ => {
            let response_status = response.status();
            let request_json = response.text().await.unwrap();
            let error_message = extract_request_error(&request_json.as_str());
            if error_message.is_none() {
                let error = f!("URL: {}\nStatus [{}]: Cannot get message", url, response_status.as_str());
                Err(error)
            }
            else{
                let error = f!("URL: {}\nStatus [{}]: {}", url, response_status.as_str(), error_message.unwrap());
                Err(error)
            }
            
        },
    }
}