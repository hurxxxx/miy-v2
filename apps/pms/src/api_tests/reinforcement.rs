//! The reinforcement (COMMON-FEATURES-PLAN §2.2 "보강", docs/PORTING.md §10):
//! milestones per list or space, custom fields with typed values, filters and
//! sorting, templates saved from tasks, and saved filters on the server.
use super::*;

fn id_of(value: &Value) -> String {
    value["id"].as_str().unwrap().to_string()
}

async fn list_tasks(user: &str, list: &str, query: &str) -> Vec<String> {
    let page = ok(
        user,
        Method::GET,
        &format!("/v1/pms/lists/{list}/tasks?page_size=100&{query}"),
        None,
    )
    .await;
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["title"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn milestones_belong_to_a_list_or_the_whole_space() {
    let f = fixture().await;
    let member = new_user("Member");
    add_member(&f.owner, &f.space, &member, "member").await;
    let outsider = new_user("Outsider");
    let other_list = id_of(&create_list(&f.owner, &f.space, &unique_key("M"), "운영").await);

    let list_milestone = ok(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/lists/{}/milestones", f.list),
        Some(json!({"title": "Beta", "due_date": "2026-12-01"})),
    )
    .await;
    assert_eq!(list_milestone["scope"], "list");
    assert_eq!(list_milestone["team_id"], f.space.as_str());
    let space_milestone = ok(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/spaces/{}/milestones", f.space),
        Some(json!({"title": "Launch", "start_date": "2026-11-01", "due_date": "2026-12-31"})),
    )
    .await;
    assert_eq!(space_milestone["scope"], "space");
    assert!(space_milestone["list_id"].is_null());

    // A list shows its own and the space-wide milestones; another list only
    // the space-wide one.
    let mine = ok(
        &member,
        Method::GET,
        &format!("/v1/pms/lists/{}/milestones", f.list),
        None,
    )
    .await;
    assert_eq!(mine["total"], 2);
    let other = ok(
        &member,
        Method::GET,
        &format!("/v1/pms/lists/{other_list}/milestones"),
        None,
    )
    .await;
    assert_eq!(other["total"], 1);
    assert_eq!(other["items"][0]["id"], space_milestone["id"]);
    let whole = ok(
        &member,
        Method::GET,
        &format!("/v1/pms/spaces/{}/milestones", f.space),
        None,
    )
    .await;
    assert_eq!(whole["total"], 2);
    let response = call(
        &outsider,
        Method::GET,
        &format!("/v1/pms/spaces/{}/milestones", f.space),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Tasks of any list in the space take the space-wide milestone; a list
    // milestone stays with its list.
    let task = create_task(
        &member,
        &other_list,
        json!({"title": "Release notes", "milestone_id": space_milestone["id"]}),
    )
    .await;
    assert_eq!(task["milestone_title"], "Launch");
    let response = call(
        &member,
        Method::POST,
        &format!("/v1/pms/lists/{other_list}/tasks"),
        Some(json!({"title": "Wrong", "milestone_id": list_milestone["id"]})),
    )
    .await;
    assert_eq!(error_code(response).await.1, "pms_milestone_wrong_list");
    ok(
        &member,
        Method::PATCH,
        &format!("/v1/pms/tasks/{}", id_of(&task)),
        Some(json!({"status": "done"})),
    )
    .await;
    let whole = ok(
        &member,
        Method::GET,
        &format!("/v1/pms/spaces/{}/milestones?sort_by=due_date", f.space),
        None,
    )
    .await;
    let launch = &whole["items"][1];
    assert_eq!(launch["title"], "Launch");
    assert_eq!(launch["task_count"], 1);
    assert_eq!(launch["completed_task_count"], 1);
    assert_eq!(launch["progress"], 1.0);

    // Managers write; a member cannot.
    let response = call(
        &member,
        Method::POST,
        &format!("/v1/pms/spaces/{}/milestones", f.space),
        Some(json!({"title": "Nope"})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (
            StatusCode::FORBIDDEN,
            "pms_space_owner_admin_required".into()
        )
    );
    let response = call(
        &member,
        Method::PATCH,
        &format!("/v1/pms/milestones/{}", id_of(&list_milestone)),
        Some(json!({"status": "active"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // An explicit null clears a date; an end before the start is refused.
    let cleared = ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/milestones/{}", id_of(&space_milestone)),
        Some(json!({"due_date": null, "status": "active"})),
    )
    .await;
    assert!(cleared["due_date"].is_null());
    assert_eq!(cleared["start_date"], "2026-11-01");
    assert_eq!(cleared["status"], "active");
    let response = call(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/milestones/{}", id_of(&space_milestone)),
        Some(json!({"due_date": "2026-10-01"})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "pms_milestone_date_range_invalid".into()
        )
    );

    // Deleting keeps the task and records the change.
    let response = call(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/milestones/{}", id_of(&space_milestone)),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let detail = ok(
        &member,
        Method::GET,
        &format!("/v1/pms/tasks/{}", id_of(&task)),
        None,
    )
    .await;
    assert!(detail["task"]["milestone_id"].is_null());
    let logs = ok(
        &member,
        Method::GET,
        &format!("/v1/pms/tasks/{}/activity-logs", id_of(&task)),
        None,
    )
    .await;
    assert!(
        logs["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|log| log["field_name"] == "milestone_id" && log["to_value"].is_null())
    );
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn custom_fields_check_values_and_filter_and_sort_tasks() {
    let f = fixture().await;
    let member = new_user("Member");
    add_member(&f.owner, &f.space, &member, "member").await;
    let viewer = new_user("Viewer");
    add_member(&f.owner, &f.space, &viewer, "viewer").await;
    let fields_path = format!("/v1/pms/lists/{}/custom-fields", f.list);
    let create = |body: Value| {
        let owner = f.owner.clone();
        let path = fields_path.clone();
        async move { ok(&owner, Method::POST, &path, Some(body)).await }
    };
    let points = create(json!({"name": "Points", "field_type": "number"})).await;
    let review = create(json!({"name": "Review", "field_type": "date"})).await;
    let tags = create(json!({"name": "Tags", "field_type": "multi_select",
        "options": ["UI", "API", "DB"]}))
    .await;
    let owner_field = create(json!({"name": "Owner", "field_type": "user"})).await;
    let note = create(json!({"name": "Note", "field_type": "text"})).await;
    assert_eq!(note["sort_order"], 4);
    let response = call(
        &f.owner,
        Method::POST,
        &fields_path,
        Some(json!({"name": "points", "field_type": "text"})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (StatusCode::CONFLICT, "pms_custom_field_name_exists".into())
    );
    let response = call(
        &f.owner,
        Method::POST,
        &fields_path,
        Some(json!({"name": "Stage", "field_type": "select"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let response = call(
        &member,
        Method::POST,
        &fields_path,
        Some(json!({"name": "Mine", "field_type": "text"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let a = id_of(&create_task(&member, &f.list, json!({"title": "Alpha"})).await);
    let b = id_of(&create_task(&member, &f.list, json!({"title": "Bravo"})).await);
    let c = id_of(&create_task(&member, &f.list, json!({"title": "Charlie"})).await);
    let set = |task: String, field: &Value, value: &str| {
        let member = member.clone();
        let body = json!({"field_id": field["id"], "value": value});
        async move {
            call(
                &member,
                Method::PUT,
                &format!("/v1/pms/tasks/{task}/custom-field-values"),
                Some(body),
            )
            .await
        }
    };
    let response = set(a.clone(), &points, "3.50").await;
    assert_eq!(response.json::<Value>().await.unwrap()["value"], "3.5");
    assert!(set(b.clone(), &points, "12").await.status().is_success());
    assert_eq!(
        error_code(set(c.clone(), &points, "many").await).await,
        (
            StatusCode::BAD_REQUEST,
            "pms_custom_field_value_invalid".into()
        )
    );
    assert!(
        set(a.clone(), &review, "2026-12-24")
            .await
            .status()
            .is_success()
    );
    let response = set(a.clone(), &tags, r#"["DB","UI"]"#).await;
    assert_eq!(
        response.json::<Value>().await.unwrap()["value"],
        r#"["UI","DB"]"#
    );
    assert!(
        set(b.clone(), &tags, r#"["API"]"#)
            .await
            .status()
            .is_success()
    );
    assert_eq!(
        error_code(set(c.clone(), &owner_field, &new_user("Stranger")).await)
            .await
            .1,
        "pms_custom_field_value_invalid"
    );
    assert!(
        set(c.clone(), &owner_field, &member)
            .await
            .status()
            .is_success()
    );
    assert!(
        set(b.clone(), &note, "Needs Review soon")
            .await
            .status()
            .is_success()
    );
    let response = call(
        &viewer,
        Method::PUT,
        &format!("/v1/pms/tasks/{a}/custom-field-values"),
        Some(json!({"field_id": note["id"], "value": "x"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Summaries carry the values; an empty value removes one.
    let page = ok(
        &viewer,
        Method::GET,
        &format!("/v1/pms/lists/{}/tasks", f.list),
        None,
    )
    .await;
    let alpha = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["id"] == a.as_str())
        .unwrap()
        .clone();
    assert_eq!(alpha["custom_fields"][id_of(&points)], "3.5");
    assert!(set(c.clone(), &owner_field, "").await.status().is_success());
    assert!(
        set(c.clone(), &owner_field, &member)
            .await
            .status()
            .is_success()
    );

    // Filters by type, and sorting with empty values last.
    let pid = id_of(&points);
    assert_eq!(
        list_tasks(&viewer, &f.list, &format!("cf_{pid}=3.5")).await,
        vec!["Alpha"]
    );
    assert_eq!(
        list_tasks(&viewer, &f.list, &format!("cf_{}=review", id_of(&note))).await,
        vec!["Bravo"]
    );
    assert_eq!(
        list_tasks(&viewer, &f.list, &format!("cf_{}=API", id_of(&tags))).await,
        vec!["Bravo"]
    );
    assert_eq!(
        list_tasks(
            &viewer,
            &f.list,
            &format!("cf_{}={member}", id_of(&owner_field))
        )
        .await,
        vec!["Charlie"]
    );
    let response = call(
        &viewer,
        Method::GET,
        &format!("/v1/pms/lists/{}/tasks?cf_{pid}=abc", f.list),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        list_tasks(&viewer, &f.list, &format!("sort_by=cf_{pid}&sort_dir=asc")).await,
        vec!["Alpha", "Bravo", "Charlie"]
    );
    assert_eq!(
        list_tasks(&viewer, &f.list, &format!("sort_by=cf_{pid}&sort_dir=desc")).await,
        vec!["Bravo", "Alpha", "Charlie"]
    );

    // Renamed options keep their values, removed ones clear them.
    let updated = ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/custom-fields/{}", id_of(&tags)),
        Some(json!({"name": "Area", "options": ["Frontend", "API"],
            "expected_options": ["UI", "API", "DB"],
            "option_renames": [{"from": "UI", "to": "Frontend"}]})),
    )
    .await;
    assert_eq!(updated["name"], "Area");
    let missing = call(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/custom-fields/{}", id_of(&tags)),
        Some(json!({"options": ["API"]})),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let stale = call(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/custom-fields/{}", id_of(&tags)),
        Some(json!({"name": "Stale", "options": ["API"], "expected_options": ["UI", "API", "DB"]})),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    let values = ok(
        &viewer,
        Method::GET,
        &format!("/v1/pms/tasks/{a}/custom-field-values"),
        None,
    )
    .await;
    assert!(
        values
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["field_id"] == tags["id"] && v["value"] == r#"["Frontend"]"#)
    );

    // Reorder, then delete with the values.
    let reordered = ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/lists/{}/custom-fields/reorder", f.list),
        Some(json!({"field_ids": [note["id"], points["id"]]})),
    )
    .await;
    let names: Vec<&str> = reordered["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| field["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Note", "Points", "Review", "Area", "Owner"]);
    let response = call(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/custom-fields/{pid}"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pms_custom_field_values WHERE field_id = $1")
            .bind(&pid)
            .fetch_one(&server().1)
            .await
            .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn templates_are_saved_from_tasks_and_create_whole_tasks() {
    let f = fixture().await;
    let member = new_user("Member");
    add_member(&f.owner, &f.space, &member, "member").await;
    let viewer = new_user("Viewer");
    add_member(&f.owner, &f.space, &viewer, "viewer").await;
    let labels = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/labels", f.list),
        None,
    )
    .await;
    let qa = labels["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|label| label["name"] == "qa")
        .unwrap()["id"]
        .clone();
    let task = create_task(
        &member,
        &f.list,
        json!({"title": "Release", "priority": "high", "label_ids": [qa]}),
    )
    .await;
    let task_id = id_of(&task);
    ok(
        &member,
        Method::POST,
        &format!("/v1/pms/tasks/{task_id}/checklist"),
        Some(json!({"text": "Tag the build"})),
    )
    .await;
    let child = create_task(
        &member,
        &f.list,
        json!({"title": "Smoke test", "parent_id": task_id, "priority": "low"}),
    )
    .await;
    ok(
        &member,
        Method::POST,
        &format!("/v1/pms/tasks/{}/checklist", id_of(&child)),
        Some(json!({"text": "Login works"})),
    )
    .await;

    let response = call(
        &viewer,
        Method::POST,
        &format!("/v1/pms/tasks/{task_id}/template"),
        Some(json!({"name": "Release"})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (
            StatusCode::FORBIDDEN,
            "pms_space_viewer_modify_denied".into()
        )
    );
    let template = ok(
        &member,
        Method::POST,
        &format!("/v1/pms/tasks/{task_id}/template"),
        Some(json!({"name": "Release checklist"})),
    )
    .await;
    assert_eq!(template["team_id"], f.space.as_str());
    assert_eq!(template["title"], "Release");
    assert_eq!(template["default_priority"], "high");
    assert_eq!(
        template["checklist_items"],
        json!([{"text": "Tag the build"}])
    );
    assert_eq!(template["labels"][0]["name"], "qa");
    assert_eq!(template["subtask_count"], 1);
    assert_eq!(template["subtasks"][0]["title"], "Smoke test");
    assert_eq!(
        template["subtasks"][0]["checklist_items"],
        json!([{"text": "Login works"}])
    );

    // Every list of the space sees it; a viewer reads it.
    let other_list = id_of(&create_list(&f.owner, &f.space, &unique_key("T"), "배포").await);
    let listed = ok(
        &viewer,
        Method::GET,
        &format!("/v1/pms/lists/{other_list}/templates"),
        None,
    )
    .await;
    assert_eq!(listed["items"][0]["id"], template["id"]);

    // A task made from it gets the checklist, the labels the list has and
    // the subtasks.
    let made = create_task(
        &member,
        &other_list,
        json!({"title": "Release 2.0", "template_id": template["id"]}),
    )
    .await;
    assert_eq!(made["subtask_count"], 1);
    assert_eq!(made["labels"][0]["name"], "qa");
    assert_eq!(made["checklist_total"], 1);
    let detail = ok(
        &member,
        Method::GET,
        &format!("/v1/pms/tasks/{}", id_of(&made)),
        None,
    )
    .await;
    let subtask = &detail["subtasks"][0];
    assert_eq!(subtask["title"], "Smoke test");
    assert_eq!(subtask["priority"], "low");
    assert_eq!(subtask["checklist_total"], 1);

    // A template of another space does not apply.
    let elsewhere = new_user("Elsewhere");
    let other_space = id_of(&create_space(&elsewhere, "다른 공간").await);
    let foreign = ok(
        &elsewhere,
        Method::POST,
        &format!("/v1/pms/spaces/{other_space}/templates"),
        Some(json!({"name": "Foreign", "subtasks": [{"title": "Step one"}]})),
    )
    .await;
    let response = call(
        &member,
        Method::POST,
        &format!("/v1/pms/lists/{other_list}/tasks"),
        Some(json!({"title": "Mixed", "template_id": foreign["id"]})),
    )
    .await;
    assert_eq!(error_code(response).await.1, "pms_template_wrong_space");
    let response = call(
        &member,
        Method::GET,
        &format!("/v1/pms/templates/{}", id_of(&foreign)),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Editors change and delete; bad content is refused.
    let edited = ok(
        &member,
        Method::PATCH,
        &format!("/v1/pms/templates/{}", id_of(&template)),
        Some(
            json!({"name": "Release v2", "subtasks": [{"title": "Smoke", "priority": "critical"}]}),
        ),
    )
    .await;
    assert_eq!(edited["subtasks"][0]["priority"], "critical");
    let response = call(
        &member,
        Method::PATCH,
        &format!("/v1/pms/templates/{}", id_of(&template)),
        Some(json!({"checklist_items": [{"text": ""}]})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let response = call(
        &viewer,
        Method::DELETE,
        &format!("/v1/pms/templates/{}", id_of(&template)),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = call(
        &member,
        Method::DELETE,
        &format!("/v1/pms/templates/{}", id_of(&template)),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn saved_filters_are_personal_shareable_and_follow_access() {
    let f = fixture().await;
    let member = new_user("Member");
    add_member(&f.owner, &f.space, &member, "member").await;
    let list_path = format!("/v1/pms/saved-filters?list_id={}", f.list);

    let private = ok(
        &f.owner,
        Method::POST,
        "/v1/pms/saved-filters",
        Some(
            json!({"list_id": f.list, "name": "Mine", "params": {"q": "secret",
            "priority": "high", "archived_state": "active"},
            "sort": {"field": "due_date", "direction": "desc"}, "group_by": "assignee"}),
        ),
    )
    .await;
    assert_eq!(
        private["params"],
        json!({"priority": "high", "archived_state": "active"})
    );
    assert_eq!(private["is_mine"], true);
    let seen = ok(&member, Method::GET, &list_path, None).await;
    assert_eq!(seen["items"], json!([]));

    // Only managers share; members keep their own filters private.
    let response = call(
        &member,
        Method::POST,
        "/v1/pms/saved-filters",
        Some(json!({"list_id": f.list, "name": "Team", "params": {}, "shared": true})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (
            StatusCode::FORBIDDEN,
            "pms_space_owner_admin_required".into()
        )
    );
    let member_own = ok(
        &member,
        Method::POST,
        "/v1/pms/saved-filters",
        Some(json!({"list_id": f.list, "name": "My open", "params": {"assignee_id": member}})),
    )
    .await;
    let response = call(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/saved-filters/{}", id_of(&member_own)),
        Some(json!({"name": "Taken"})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (StatusCode::NOT_FOUND, "pms_saved_filter_not_found".into())
    );
    let shared = ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/saved-filters/{}", id_of(&private)),
        Some(json!({"shared": true, "name": "Team high"})),
    )
    .await;
    assert_eq!(shared["shared"], true);
    let seen = ok(&member, Method::GET, &list_path, None).await;
    let names: Vec<&str> = seen["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["My open", "Team high"]);
    let team = seen["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "Team high")
        .unwrap();
    assert_eq!(team["can_manage"], false);
    assert_eq!(team["owner_name"], "홍길동");
    assert_eq!(team["group_by"], "assignee");
    let response = call(
        &member,
        Method::DELETE,
        &format!("/v1/pms/saved-filters/{}", id_of(&private)),
        None,
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (
            StatusCode::FORBIDDEN,
            "pms_saved_filter_manage_denied".into()
        )
    );

    // The member opens the list with the shared filter until it is unshared.
    let defaults = ok(
        &member,
        Method::PUT,
        "/v1/pms/saved-filters/default",
        Some(json!({"list_id": f.list, "filter_id": private["id"]})),
    )
    .await;
    assert_eq!(defaults["default_filter_id"], private["id"]);
    ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/saved-filters/{}", id_of(&private)),
        Some(json!({"shared": false})),
    )
    .await;
    let seen = ok(&member, Method::GET, &list_path, None).await;
    assert!(seen["default_filter_id"].is_null());
    assert_eq!(seen["items"].as_array().unwrap().len(), 1);

    // The space's combined view keeps its own filters.
    let space_filter = ok(
        &member,
        Method::POST,
        "/v1/pms/saved-filters",
        Some(json!({"space_id": f.space, "name": "Space view", "params": {"status": ["todo"]}})),
    )
    .await;
    assert!(space_filter["list_id"].is_null());
    let in_space = ok(
        &member,
        Method::GET,
        &format!("/v1/pms/saved-filters?space_id={}", f.space),
        None,
    )
    .await;
    assert_eq!(in_space["items"].as_array().unwrap().len(), 1);

    // Bad content, and access that ends.
    let response = call(
        &member,
        Method::POST,
        "/v1/pms/saved-filters",
        Some(json!({"list_id": f.list, "name": "Bad", "params": {"color": "red"}})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let response = call(
        &member,
        Method::POST,
        "/v1/pms/saved-filters",
        Some(json!({"list_id": f.list, "space_id": f.space, "name": "Both", "params": {}})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    ok(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/spaces/{}/members/{member}", f.space),
        None,
    )
    .await;
    let response = call(&member, Method::GET, &list_path, None).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = call(
        &member,
        Method::DELETE,
        &format!("/v1/pms/saved-filters/{}", id_of(&member_own)),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn reinforcement_writes_check_the_role_after_the_space_lock() {
    let f = fixture().await;
    let admin = new_user("Admin");
    add_member(&f.owner, &f.space, &admin, "admin").await;

    for (path, body) in [
        (
            format!("/v1/pms/lists/{}/custom-fields", f.list),
            json!({"name": "Late", "field_type": "text"}),
        ),
        (
            format!("/v1/pms/spaces/{}/milestones", f.space),
            json!({"title": "Late"}),
        ),
    ] {
        let mut tx = server().1.begin().await.unwrap();
        let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SELECT id FROM pms_spaces WHERE id = $1 FOR UPDATE")
            .bind(&f.space)
            .execute(&mut *tx)
            .await
            .unwrap();
        let request = tokio::spawn({
            let admin = admin.clone();
            let path = path.clone();
            async move { call(&admin, Method::POST, &path, Some(body)).await }
        });
        wait_for_query_blocked_by(blocker, "%FROM pms_spaces%FOR SHARE%").await;
        sqlx::query(
            "UPDATE pms_space_members SET role = 'member' WHERE space_id = $1 AND user_id = $2",
        )
        .bind(&f.space)
        .bind(&admin)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            request.await.unwrap().status(),
            StatusCode::FORBIDDEN,
            "{path}"
        );
        sqlx::query(
            "UPDATE pms_space_members SET role = 'admin' WHERE space_id = $1 AND user_id = $2",
        )
        .bind(&f.space)
        .bind(&admin)
        .execute(&server().1)
        .await
        .unwrap();
    }
    let fields: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pms_custom_fields WHERE list_id = $1")
            .bind(&f.list)
            .fetch_one(&server().1)
            .await
            .unwrap();
    assert_eq!(fields, 0);
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn copies_keep_space_milestones_and_typed_field_values_that_fit() {
    let f = fixture().await;
    let member = new_user("Member");
    add_member(&f.owner, &f.space, &member, "member").await;
    let target = id_of(&create_list(&f.owner, &f.space, &unique_key("C"), "복사 대상").await);
    let milestone = ok(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/spaces/{}/milestones", f.space),
        Some(json!({"title": "Launch"})),
    )
    .await;
    let mut fields = Vec::new();
    for list in [&f.list, &target] {
        let owner_field = ok(
            &f.owner,
            Method::POST,
            &format!("/v1/pms/lists/{list}/custom-fields"),
            Some(json!({"name": "Owner", "field_type": "user"})),
        )
        .await;
        let tags = ok(
            &f.owner,
            Method::POST,
            &format!("/v1/pms/lists/{list}/custom-fields"),
            Some(json!({"name": "Tags", "field_type": "multi_select",
                "options": if list == &target { json!(["UI"]) } else { json!(["UI", "API"]) }})),
        )
        .await;
        fields.push((owner_field, tags));
    }
    let task = create_task(
        &member,
        &f.list,
        json!({"title": "Copied", "milestone_id": milestone["id"]}),
    )
    .await;
    let task_id = id_of(&task);
    for (field, value) in [
        (&fields[0].0, member.clone()),
        (&fields[0].1, r#"["UI","API"]"#.to_string()),
    ] {
        ok(
            &member,
            Method::PUT,
            &format!("/v1/pms/tasks/{task_id}/custom-field-values"),
            Some(json!({"field_id": field["id"], "value": value})),
        )
        .await;
    }
    let result = ok(
        &member,
        Method::POST,
        &format!("/v1/pms/lists/{}/tasks/copy", f.list),
        Some(json!({"task_ids": [task_id], "target_list_id": target})),
    )
    .await;
    // The space milestone and the member carry over; API is not an option of
    // the target's Tags field, so that value is counted as unmatched.
    assert_eq!(
        result["unmatched"],
        json!({"assignees": 0, "labels": 0, "milestones": 0, "custom_fields": 1})
    );
    let copied = ok(
        &member,
        Method::GET,
        &format!("/v1/pms/lists/{target}/tasks"),
        None,
    )
    .await;
    let copy = &copied["items"][0];
    assert_eq!(copy["milestone_id"], milestone["id"]);
    assert_eq!(copy["custom_fields"][id_of(&fields[1].0)], member.as_str());
    assert!(copy["custom_fields"].get(id_of(&fields[1].1)).is_none());
}
