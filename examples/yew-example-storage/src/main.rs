#![recursion_limit = "1024"]

use console_error_panic_hook::set_once as set_panic_hook;
use once_cell::sync::Lazy;
use ybc::TileCtx::{Child, Parent};
use ybc::InputType::Text;
use yew::prelude::*;
use tinysearch::{Filters, Storage,search};

struct App{
    value: String,
    posts: Vec<String>,
    link: ComponentLink<App>
}

enum Msg {
    SearchChanged(String)
}

static FILTERS: Lazy<Filters> = Lazy::new(|| {
    let bytes = include_bytes!("storage");
    Storage::from_bytes(bytes).unwrap().filters
});

impl App{
    fn render_post(s: &String) -> yew::Html{
        html!{
            <ybc::Tile ctx=Parent>
                <ybc::Tile ctx=Child classes=classes!("notification", "is-success")>
                    <ybc::Subtitle size=ybc::HeaderSize::Is3 classes=classes!("has-text-white")>{s.clone()}</ybc::Subtitle>
                </ybc::Tile>
            </ybc::Tile>
        }
    }
}

impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self{
            value: String::default(),
            posts: Vec::new(),
            link: link
        }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        match msg {
            Msg::SearchChanged(s) => {
                if s != self.value{
                    self.value = s;
                    let posts = search(&FILTERS, self.value.clone(), 5);
                    self.posts = posts.iter().map(|x|x.0.clone()).collect();
                    true
                }else{
                    false
                }
            }
        }
    }

    fn change(&mut self, _: Self::Properties) -> bool {
        false
    }

    fn view(&self) -> Html {
        html! {
            <>
            <ybc::Navbar
                classes=classes!("is-success")
                padded=true
                navbrand=html!{
                    <ybc::NavbarItem>
                        <ybc::Title classes=classes!("has-text-white") size=ybc::HeaderSize::Is4>{"Tinysearch | Trunk | Yew | YBC | demo"}</ybc::Title>
                    </ybc::NavbarItem>
                }
                navstart=html!{}
                navend=html!{
                    <>
                    <ybc::NavbarItem>
                        <ybc::Input 
                            name="as" value={self.value.clone()}
                            update=self.link.callback(|s:String| Msg::SearchChanged(s))
                            r#type=Text ></ybc::Input>
                    </ybc::NavbarItem>
                    </>
                }
            />
            

            <ybc::Hero
                classes=classes!("is-light")
                size=ybc::HeroSize::FullheightWithNavbar
                body=html!{
                    <ybc::Container>
                        <ybc::Tile ctx=Parent size=ybc::TileSize::Twelve vertical=true>
                        {
                            for self.posts.iter().map(App::render_post)
                        }
                        </ybc::Tile>
                    </ybc::Container>
                }>
            </ybc::Hero>
            </>
        }
    }
}

fn main() {
    set_panic_hook();

    yew::start_app::<App>();
}