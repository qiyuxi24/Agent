import { createRoot } from "react-dom/client";
import PetApp from "./PetApp";
import "./styles.css";

const el = document.getElementById("pet-root");
if (el) {
  createRoot(el).render(<PetApp />);
}
