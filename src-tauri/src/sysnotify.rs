//! macOS system notifications with an inline text reply, through the public
//! UserNotifications framework. Only available when running from a real
//! `.app` bundle; otherwise callers fall back to the notch panel.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_foundation::{NSArray, NSBundle, NSError, NSObject, NSObjectProtocol, NSSet, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationAction,
    UNNotificationActionOptions, UNNotificationCategory, UNNotificationCategoryOptions,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNNotificationSound, UNTextInputNotificationAction, UNTextInputNotificationResponse,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;

const CATEGORY: &str = "notchat.message";
const ACTION_REPLY: &str = "notchat.reply";
const ACTION_DEFAULT: &str = "com.apple.UNNotificationDefaultActionIdentifier";

static READY: AtomicBool = AtomicBool::new(false);

/// What the user did with a notification.
pub enum Response {
    Reply { chat_jid: String, text: String },
    Open { chat_jid: String },
}

struct Ivars {
    app: AppHandle,
    on_response: fn(&AppHandle, Response),
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and this type has no Drop impl.
    #[unsafe(super(NSObject))]
    #[name = "NotchatNotificationDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            // Show banners even while Notchat is the active app.
            handler.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            handler: &block2::DynBlock<dyn Fn()>,
        ) {
            let chat_jid = response
                .notification()
                .request()
                .content()
                .threadIdentifier()
                .to_string();
            let action = response.actionIdentifier().to_string();
            let ivars = self.ivars();
            if action == ACTION_REPLY {
                if let Some(input) = response.downcast_ref::<UNTextInputNotificationResponse>() {
                    let text = input.userText().to_string();
                    (ivars.on_response)(&ivars.app, Response::Reply { chat_jid, text });
                }
            } else if action == ACTION_DEFAULT {
                (ivars.on_response)(&ivars.app, Response::Open { chat_jid });
            }
            handler.call(());
        }
    }
);

impl Delegate {
    fn new(app: AppHandle, on_response: fn(&AppHandle, Response)) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars { app, on_response });
        // SAFETY: `init` on a freshly allocated NSObject subclass with ivars set.
        unsafe { msg_send![super(this), init] }
    }
}

/// True when the process runs from an app bundle, which the framework needs.
pub fn available() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
}

pub fn ready() -> bool {
    READY.load(Ordering::Relaxed)
}

/// Registers the delegate and the reply action, and asks for permission.
/// `reply_title`/`placeholder`/`send` are the localized labels.
pub fn setup(
    app: &AppHandle,
    on_response: fn(&AppHandle, Response),
    reply_title: &str,
    send: &str,
    placeholder: &str,
) {
    if !available() || ready() {
        return;
    }
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let delegate = Delegate::new(app.clone(), on_response);
    center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    // The centre keeps a weak reference; the delegate lives for the whole run.
    std::mem::forget(delegate);

    let reply = UNTextInputNotificationAction::actionWithIdentifier_title_options_textInputButtonTitle_textInputPlaceholder(
        &NSString::from_str(ACTION_REPLY),
        &NSString::from_str(reply_title),
        UNNotificationActionOptions::empty(),
        &NSString::from_str(send),
        &NSString::from_str(placeholder),
    );
    let reply: Retained<UNNotificationAction> = Retained::into_super(reply);
    let category = UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
        &NSString::from_str(CATEGORY),
        &NSArray::from_retained_slice(&[reply]),
        &NSArray::new(),
        UNNotificationCategoryOptions::empty(),
    );
    center.setNotificationCategories(&NSSet::from_retained_slice(&[category]));

    let granted = RcBlock::new(|ok: Bool, _error: *mut NSError| {
        READY.store(ok.as_bool(), Ordering::Relaxed);
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &granted,
    );
}

/// Posts a notification for an incoming message.
pub fn post(id: &str, chat_jid: &str, title: &str, body: &str) {
    if !available() {
        return;
    }
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    content.setThreadIdentifier(&NSString::from_str(chat_jid));
    content.setCategoryIdentifier(&NSString::from_str(CATEGORY));
    content.setSound(Some(&UNNotificationSound::defaultSound()));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(id),
        &content,
        None,
    );
    UNUserNotificationCenter::currentNotificationCenter()
        .addNotificationRequest_withCompletionHandler(&request, None);
}
